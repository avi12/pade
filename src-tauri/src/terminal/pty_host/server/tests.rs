use super::*;
use crate::terminal::session::registry::SessionError;

type Link = (
    BufReader<tokio::io::ReadHalf<transport::ClientStream>>,
    tokio::io::WriteHalf<transport::ClientStream>,
);

/// Connect, send `opening`, and read the greeting (`None`: dropped unanswered).
async fn open(endpoint: &str, opening: &Opening) -> (Option<Greeting>, Link) {
    let stream = transport::connect(endpoint).await.expect("connected");
    let (read_half, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    write_frame(&mut writer, opening).await.expect("sent");
    let mut buffer = Vec::new();
    let greeting = read_frame(&mut reader, &mut buffer)
        .await
        .unwrap_or_default();
    (greeting, (reader, writer))
}

/// The whole handshake against a real host on a real pipe, in-process: a
/// wrong token gets no answer, the right one attaches, a second app is
/// turned away while the first is attached, and a retire ends the host.
#[test]
fn the_handshake_guards_the_host_and_a_retire_ends_it() {
    let root = std::env::temp_dir().join(format!("pade-pty-host-handshake-{}", std::process::id()));
    let paths = HostPaths::under(&root, "instance");
    std::fs::create_dir_all(paths.instance_directory()).expect("instance directory");
    let log = Arc::new(HostLog::start(paths.log_file()));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");

    runtime.block_on(async {
        let serving = {
            let paths = paths.clone();
            tokio::spawn(async move { serve(&paths, &log).await })
        };
        let state = loop {
            if let Some(state) = read_state(&paths.state_file()) {
                break state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let hello = |token: &str| Opening::Hello {
            protocol_version: PROTOCOL_VERSION,
            token: token.to_string(),
        };

        let (refused, _) = open(&state.endpoint, &hello("wrong")).await;
        assert_eq!(refused, None, "a wrong token gets no answer");

        let (welcome, attached) = open(&state.endpoint, &hello(&state.token)).await;
        assert!(
            matches!(
                welcome,
                Some(Greeting::Welcome {
                    session_count: 0,
                    ..
                })
            ),
            "{welcome:?}"
        );
        let (busy, _) = open(&state.endpoint, &hello(&state.token)).await;
        assert_eq!(busy, Some(Greeting::Busy));

        let (mismatch, _) = open(
            &state.endpoint,
            &Opening::Hello {
                protocol_version: PROTOCOL_VERSION + 1,
                token: state.token.clone(),
            },
        )
        .await;
        assert_eq!(
            mismatch,
            Some(Greeting::VersionMismatch {
                protocol_version: PROTOCOL_VERSION
            })
        );

        drop(attached);
        let retire = Opening::Retire {
            token: state.token.clone(),
        };
        let retired = loop {
            let (greeting, _) = open(&state.endpoint, &retire).await;
            if greeting != Some(Greeting::Busy) {
                break greeting;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(retired, Some(Greeting::Retired));
        tokio::time::timeout(Duration::from_secs(10), serving)
            .await
            .expect("the host stops serving")
            .expect("serve task");
    });
    let _ = std::fs::remove_dir_all(&root);
}

fn idle_registry() -> Arc<SessionRegistry> {
    SessionRegistry::new(Arc::new(HostEvents::default()))
}

#[test]
fn requests_against_unknown_sessions_answer_like_the_app_always_did() {
    let registry = idle_registry();
    assert_eq!(
        outcome_of(
            &registry,
            Request::Transcript {
                owner: "main".into(),
                id: "nobody".into()
            }
        ),
        Outcome::Transcript {
            text: String::new()
        }
    );
    assert_eq!(
        outcome_of(&registry, Request::List { owner: None }),
        Outcome::Sessions {
            sessions: Vec::new()
        }
    );
    assert_eq!(
        outcome_of(
            &registry,
            Request::Transfer {
                from: "main".into(),
                to: "w-1".into(),
                ids: vec!["gone".into()]
            }
        ),
        Outcome::Failed {
            error: SessionError::Missing { id: "gone".into() }
        }
    );
}

#[test]
fn one_app_attaches_at_a_time() {
    let host = Host {
        registry: idle_registry(),
        events: Arc::new(HostEvents::default()),
        token: "t".into(),
        executable_fingerprint: String::new(),
        attached: Mutex::new(None),
        shutdown: Notify::new(),
        log: Arc::new(HostLog {
            path: std::env::temp_dir().join("pade-pty-host-test.log"),
        }),
    };
    assert!(host.try_attach(1));
    assert!(!host.try_attach(2), "a second app is turned away");
    host.detach(2);
    assert!(host.is_attached(), "only the attached connection detaches");
    host.detach(1);
    assert!(host.try_attach(2));
}
