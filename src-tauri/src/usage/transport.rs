//! The one curl transport behind every vendor's usage call: the config document, the header-token guard and the status mapping.

use super::readings::RequestFailure;
use std::fmt::Write as _;
use std::io::Write;
use std::process::Stdio;

/// Whether `token` is safe to interpolate into a quoted `header = "…"` directive
/// of a curl `--config` file.
///
/// The token is spliced verbatim inside the double quotes, so a stray quote or
/// newline could close the string and inject an arbitrary curl directive
/// (config-file directive injection). We therefore accept only the OAuth-token
/// character set - non-empty, ASCII alphanumerics plus `.`, `_`, `-` - and
/// reject everything else.
pub(crate) fn is_safe_header_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(super) const AUTHORIZATION_HEADER: &str = "Authorization";
pub(super) const USER_AGENT_HEADER: &str = "User-Agent";

/// One HTTP request, described as a curl `--config` document.
pub(super) struct CurlRequest<'request> {
    pub(super) url: &'request str,
    pub(super) timeout_seconds: u32,
    /// `(name, value)` pairs, each already checked safe to quote.
    pub(super) headers: &'request [(&'request str, &'request str)],
    /// A JSON body, which turns the request into a POST.
    pub(super) json_body: Option<&'request str>,
}

/// The curl `--config` document for `request`. `write-out` appends the HTTP
/// status on a final line of its own, so [`http_answer`] can tell a rejected
/// sign-in from a refusal and never reads an error body as an answer.
pub(super) fn curl_config(request: &CurlRequest) -> String {
    let CurlRequest {
        url,
        timeout_seconds,
        headers,
        json_body,
    } = request;
    let mut config = format!(
        "silent\nshow-error\nwrite-out = \"\\n%{{http_code}}\"\nmax-time = {timeout_seconds}\nurl = \"{url}\"\n"
    );
    // Writing into a `String` cannot fail, so the `fmt::Result`s carry nothing.
    for (name, value) in *headers {
        let _ = writeln!(config, "header = \"{name}: {value}\"");
    }
    if let Some(body) = json_body {
        let quoted = body.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = write!(config, "request = \"POST\"\ndata = \"{quoted}\"\n");
    }
    config
}

/// Run `request` by feeding its curl `--config` document on stdin - so the
/// bearer token never appears in the process arguments. The one transport
/// behind every vendor's usage call and the limit-reset calls.
pub(super) fn curl_json(request: &CurlRequest) -> Result<String, RequestFailure> {
    let config = curl_config(request);
    let mut child = crate::util::command("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| RequestFailure::Unreachable)?;

    // Write the config, then drop stdin so curl sees EOF and runs the request.
    child
        .stdin
        .take()
        .ok_or(RequestFailure::Unreachable)?
        .write_all(config.as_bytes())
        .map_err(|_| RequestFailure::Unreachable)?;

    let output = child
        .wait_with_output()
        .map_err(|_| RequestFailure::Unreachable)?;
    // A transport failure (no network, a timeout) exits non-zero.
    if !output.status.success() {
        return Err(RequestFailure::Unreachable);
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| RequestFailure::Unreachable)?;
    http_answer(&stdout)
}

/// HTTP statuses that mean the vendor refused the credentials themselves.
const UNAUTHORIZED_STATUS: u16 = 401;
const FORBIDDEN_STATUS: u16 = 403;

/// curl's output split into the body and the status `write-out` appended on the
/// last line: the body on a success, otherwise what the status says - a 401 or
/// 403 is a rejected sign-in, anything else (a 429, a server error, no status
/// at all) leaves the vendor unreachable for now.
pub(super) fn http_answer(output: &str) -> Result<String, RequestFailure> {
    let (body, status) = output.rsplit_once('\n').unwrap_or(("", output));
    let status: u16 = status
        .trim()
        .parse()
        .map_err(|_| RequestFailure::Unreachable)?;
    match status {
        200..=299 => Ok(body.to_string()),
        UNAUTHORIZED_STATUS | FORBIDDEN_STATUS => Err(RequestFailure::Rejected),
        _ => Err(RequestFailure::Unreachable),
    }
}
