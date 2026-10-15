// The terminal color-scheme notification Claude Code subscribes to with DECSET
// 2031. xterm paints its new palette immediately, but changing `options.theme`
// does not emit this application-facing status report by itself.

import { Scheme } from "@/lib/types";

const CONTROL_SEQUENCE_INTRODUCER = "\x1b[";
const COLOR_SCHEME_STATUS = "?997";
const COLOR_SCHEME_NOTIFICATIONS = "?2031h";
const DEVICE_STATUS_REPORT = "n";
const DARK_SCHEME_CODE = 1;
const LIGHT_SCHEME_CODE = 2;

/** Report the current terminal color preference to a TUI that opted into DECSET
 * 2031 notifications. */
export function colorSchemeReport(scheme: Scheme) {
  const code = scheme === Scheme.enum.dark ? DARK_SCHEME_CODE : LIGHT_SCHEME_CODE;
  return `${CONTROL_SEQUENCE_INTRODUCER}${COLOR_SCHEME_STATUS};${code}${DEVICE_STATUS_REPORT}`;
}

/** Whether a TUI chunk enables the DEC 2031 color-scheme notification channel. */
export function enablesColorSchemeNotifications(data: string) {
  return data.includes(`${CONTROL_SEQUENCE_INTRODUCER}${COLOR_SCHEME_NOTIFICATIONS}`);
}
