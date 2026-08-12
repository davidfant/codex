use anyhow::Result;
use codex_core::compact::SUMMARIZATION_PROMPT;
use codex_model_provider_info::built_in_model_providers;
use codex_protocol::items::TurnItem;
use codex_protocol::protocol::AdditionalContextEntry;
use codex_protocol::protocol::AdditionalContextKind;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::Op;
use codex_protocol::user_input::UserInput;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::context_snapshot::ContextSnapshotRenderMode;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event_match;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn additional_context_is_model_visible_but_not_a_user_message_item() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "inspect the active tab".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "browser_info".to_string(),
                    AdditionalContextEntry {
                        value: "tab one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: "run one".to_string(),
                        kind: AdditionalContextKind::Application,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;

    let user_item = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::ItemCompleted(ItemCompletedEvent {
            item: TurnItem::UserMessage(item),
            ..
        }) => Some(item.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        user_item.content,
        vec![UserInput::Text {
            text: "inspect the active tab".to_string(),
            text_elements: Vec::new(),
        }]
    );
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    let request = request.single_request();
    insta::assert_snapshot!(
        "additional_context_simple_input",
        context_snapshot::format_labeled_requests_snapshot(
            "additional context is inserted before the user turn input.",
            &[("Request", &request)],
            &ContextSnapshotOptions::default()
                .strip_capability_instructions()
                .render_mode(ContextSnapshotRenderMode::KindWithTextPrefix { max_chars: 160 }),
        )
    );
    let developer_context_texts = request
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with("<automation_info>"))
        .collect::<Vec<_>>();
    assert_eq!(
        developer_context_texts,
        vec!["<automation_info>run one</automation_info>"]
    );
    assert_eq!(
        request.message_input_texts("user"),
        vec![
            "<external_browser_info>tab one</external_browser_info>",
            "inspect the active tab",
        ]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_context_like_user_text_remains_a_user_message_item() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;
    let user_input = UserInput::Text {
        text: "<external_api>".to_string(),
        text_elements: Vec::new(),
    };

    test.codex
        .submit(Op::UserInput {
            items: vec![user_input.clone()],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::new(),
            thread_settings: Default::default(),
        })
        .await?;

    let user_item = wait_for_event_match(&test.codex, |event| match event {
        EventMsg::ItemCompleted(ItemCompletedEvent {
            item: TurnItem::UserMessage(item),
            ..
        }) => Some(item.clone()),
        _ => None,
    })
    .await;
    assert_eq!(user_item.content, vec![user_input]);
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    let request = request.single_request();
    assert_eq!(request.message_input_texts("user"), vec!["<external_api>"]);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn additional_context_trust_controls_message_role() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "inspect context".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "browser_info".to_string(),
                    AdditionalContextEntry {
                        value: "tab one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: "run one".to_string(),
                        kind: AdditionalContextKind::Application,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    let request = request.single_request();
    let developer_context_texts = request
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with("<automation_info>"))
        .collect::<Vec<_>>();
    assert_eq!(
        developer_context_texts,
        vec!["<automation_info>run one</automation_info>"]
    );
    assert_eq!(
        request.message_input_texts("user"),
        vec![
            "<external_browser_info>tab one</external_browser_info>",
            "inspect context",
        ]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn additional_context_is_deduplicated_between_turns_while_retained() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let first_request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let second_request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;
    let additional_context = BTreeMap::from([(
        "browser_info".to_string(),
        AdditionalContextEntry {
            value: "same tab".to_string(),
            kind: AdditionalContextKind::Untrusted,
        },
    )]);

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "first turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: additional_context.clone(),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "second turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context,
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    assert_eq!(
        first_request.single_request().message_input_texts("user"),
        vec![
            "<external_browser_info>same tab</external_browser_info>",
            "first turn",
        ]
    );
    assert_eq!(
        second_request.single_request().message_input_texts("user"),
        vec![
            "<external_browser_info>same tab</external_browser_info>",
            "first turn",
            "second turn",
        ]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unchanged_additional_context_survives_automatic_compaction() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let requests = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_function_call(
                    "call-1",
                    "shell_command",
                    &json!({ "command": "echo compact", "login": false }).to_string(),
                ),
                ev_completed_with_tokens("resp-1", /*total_tokens*/ 100),
            ]),
            sse(vec![
                ev_response_created("resp-compact"),
                ev_assistant_message("msg-compact", "compacted history"),
                ev_completed_with_tokens("resp-compact", /*total_tokens*/ 80_000),
            ]),
            sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
        ],
    )
    .await;
    let mut model_provider = built_in_model_providers(/*openai_base_url*/ None)["openai"].clone();
    model_provider.name = "OpenAI (test)".to_string();
    model_provider.base_url = Some(format!("{}/v1", server.uri()));
    model_provider.supports_websockets = false;
    let test = test_codex()
        .with_config(move |config| {
            config.include_environment_context = false;
            config.model_provider = model_provider;
            config.compact_prompt = Some(SUMMARIZATION_PROMPT.to_string());
            config.model_context_window = Some(100);
            config.model_auto_compact_token_limit = Some(200);
        })
        .build(&server)
        .await?;
    let additional_context = BTreeMap::from([(
        "browser_access".to_string(),
        AdditionalContextEntry {
            value: "profile id: browser-profile-1".to_string(),
            kind: AdditionalContextKind::Application,
        },
    )]);

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "browse with the listed profile".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context,
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    let requests = requests.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests[1]
            .message_input_texts("user")
            .iter()
            .any(|text| text == SUMMARIZATION_PROMPT),
        "second request was not compaction"
    );
    let post_compaction_browser_context = requests[2]
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with("<browser_access>"))
        .collect::<Vec<_>>();
    assert_eq!(
        post_compaction_browser_context,
        vec!["<browser_access>profile id: browser-profile-1</browser_access>"]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn additional_context_removes_one_value_while_adding_another() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let first_request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let second_request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
    )
    .await;
    let third_request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-3"), ev_completed("resp-3")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "first turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: "run one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "browser_info".to_string(),
                    AdditionalContextEntry {
                        value: "tab one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "second turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: "run one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "terminal_info".to_string(),
                    AdditionalContextEntry {
                        value: "pty one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "third turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: "run one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "browser_info".to_string(),
                    AdditionalContextEntry {
                        value: "tab one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
                (
                    "terminal_info".to_string(),
                    AdditionalContextEntry {
                        value: "pty one".to_string(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    assert_eq!(
        first_request.single_request().message_input_texts("user"),
        vec![
            "<external_automation_info>run one</external_automation_info>",
            "<external_browser_info>tab one</external_browser_info>",
            "first turn",
        ]
    );
    assert_eq!(
        second_request.single_request().message_input_texts("user"),
        vec![
            "<external_automation_info>run one</external_automation_info>",
            "<external_browser_info>tab one</external_browser_info>",
            "first turn",
            "<external_terminal_info>pty one</external_terminal_info>",
            "second turn",
        ]
    );
    assert_eq!(
        third_request.single_request().message_input_texts("user"),
        vec![
            "<external_automation_info>run one</external_automation_info>",
            "<external_browser_info>tab one</external_browser_info>",
            "first turn",
            "<external_terminal_info>pty one</external_terminal_info>",
            "second turn",
            "<external_browser_info>tab one</external_browser_info>",
            "third turn",
        ]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn additional_context_values_are_truncated_before_model_input() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const MAX_EXPECTED_EXTERNAL_CONTEXT_TEXT_BYTES: usize = 5 * 1024;

    let server = start_mock_server().await;
    let request = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let test = test_codex()
        .with_config(|config| config.include_environment_context = false)
        .build(&server)
        .await?;
    let long_browser_value = format!("browser-head-{}browser-tail", "b".repeat(40_000));
    let long_automation_value = format!("automation-head-{}automation-tail", "a".repeat(40_000));
    let untruncated_browser_fragment =
        format!("<external_browser_info>{long_browser_value}</external_browser_info>");
    let untruncated_automation_fragment =
        format!("<automation_info>{long_automation_value}</automation_info>");

    test.codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "summarize context".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: BTreeMap::from([
                (
                    "automation_info".to_string(),
                    AdditionalContextEntry {
                        value: long_automation_value.clone(),
                        kind: AdditionalContextKind::Application,
                    },
                ),
                (
                    "browser_info".to_string(),
                    AdditionalContextEntry {
                        value: long_browser_value.clone(),
                        kind: AdditionalContextKind::Untrusted,
                    },
                ),
            ]),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event_match(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_)).then_some(())
    })
    .await;

    let request = request.single_request();
    let developer_texts = request
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with("<automation_info>"))
        .collect::<Vec<_>>();
    let [automation_text] = developer_texts.as_slice() else {
        panic!("expected application additional context, got {developer_texts:?}");
    };
    assert!(automation_text.starts_with(&format!(
        "<automation_info>automation-head-{}",
        "a".repeat(1024)
    )));
    assert!(automation_text.contains("tokens truncated"));
    assert!(automation_text.ends_with("automation-tail</automation_info>"));
    assert!(automation_text.len() < untruncated_automation_fragment.len());
    assert!(
        automation_text.len() <= MAX_EXPECTED_EXTERNAL_CONTEXT_TEXT_BYTES,
        "application additional context was not capped before model input: {} bytes",
        automation_text.len()
    );

    let user_texts = request.message_input_texts("user");
    let [external_text, user_text] = user_texts.as_slice() else {
        panic!("expected external context plus user input, got {user_texts:?}");
    };
    assert_eq!(user_text, "summarize context");
    assert!(external_text.starts_with(&format!(
        "<external_browser_info>browser-head-{}",
        "b".repeat(1024)
    )));
    assert!(external_text.contains("tokens truncated"));
    assert!(external_text.ends_with("browser-tail</external_browser_info>"));
    assert!(external_text.len() < untruncated_browser_fragment.len());
    assert!(
        external_text.len() <= MAX_EXPECTED_EXTERNAL_CONTEXT_TEXT_BYTES,
        "untrusted additional context was not capped before model input: {} bytes",
        external_text.len()
    );

    Ok(())
}
