use teloxide::types::{ChatKind, Message, PublicChatKind, Update, UpdateKind};

use crate::routing::{TelegramChatKind, TelegramEnvelope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingTextMessage {
    pub envelope: TelegramEnvelope,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTarget {
    pub chat_id: i64,
    pub message_thread_id: Option<i64>,
}

pub fn extract_text_message(update: &Update) -> Option<IncomingTextMessage> {
    let message = match &update.kind {
        UpdateKind::Message(message) => message,
        _ => return None,
    };
    let text = message_text(message)?;
    if text.trim().is_empty() {
        return None;
    }
    Some(IncomingTextMessage {
        envelope: envelope_from_message(message),
        text: text.to_owned(),
    })
}

pub fn reply_target(envelope: &TelegramEnvelope) -> ReplyTarget {
    ReplyTarget {
        chat_id: envelope.chat_id,
        message_thread_id: envelope.message_thread_id,
    }
}

fn envelope_from_message(message: &Message) -> TelegramEnvelope {
    TelegramEnvelope {
        chat_id: message.chat.id.0,
        chat_kind: map_chat_kind(&message.chat.kind),
        is_forum: is_forum_chat(&message.chat.kind),
        message_thread_id: message.thread_id.map(|thread| i64::from(thread.0.0)),
    }
}

fn map_chat_kind(kind: &ChatKind) -> TelegramChatKind {
    match kind {
        ChatKind::Private(_) => TelegramChatKind::Direct,
        ChatKind::Public(public) => match public.kind {
            PublicChatKind::Group => TelegramChatKind::Group,
            PublicChatKind::Supergroup(_) | PublicChatKind::Channel(_) => {
                TelegramChatKind::Supergroup
            }
        },
    }
}

fn is_forum_chat(kind: &ChatKind) -> bool {
    match kind {
        ChatKind::Public(public) => match &public.kind {
            PublicChatKind::Supergroup(supergroup) => supergroup.is_forum,
            _ => false,
        },
        ChatKind::Private(_) => false,
    }
}

fn message_text(message: &Message) -> Option<&str> {
    message.text().or_else(|| message.caption())
}

#[cfg(test)]
mod tests {
    use teloxide::types::Update;

    use crate::update_adapter::{TelegramChatKind, extract_text_message, reply_target};

    fn parse_update(text: &str) -> Update {
        serde_json::from_str(text).expect("valid telegram update fixture")
    }

    #[test]
    fn extracts_forum_topic_text_message() {
        let update = parse_update(
            r#"{
                "message": {
                    "chat": {
                        "id": -1001293752024,
                        "title": "CryptoInside Chat",
                        "type": "supergroup",
                        "username": "cryptoinside_talk",
                        "is_forum": true
                    },
                    "date": 1721592580,
                    "entities": [
                        {
                            "length": 7,
                            "offset": 0,
                            "type": "bot_command"
                        }
                    ],
                    "from": {
                        "first_name": "the Cable Guy",
                        "id": 5964236329,
                        "is_bot": false,
                        "language_code":"en",
                        "username": "spacewhaleblues"
                    },
                    "message_id": 134546,
                    "message_thread_id": 134545,
                    "text": "/report"
                },
                "update_id": 439432600
            }"#,
        );

        let incoming = extract_text_message(&update).expect("message extracted");
        assert_eq!(incoming.text, "/report");
        assert_eq!(incoming.envelope.chat_kind, TelegramChatKind::Supergroup);
        assert!(incoming.envelope.is_forum);
        assert_eq!(incoming.envelope.message_thread_id, Some(134545));

        let target = reply_target(&incoming.envelope);
        assert_eq!(target.chat_id, -1001293752024_i64);
        assert_eq!(target.message_thread_id, Some(134545));
    }

    #[test]
    fn extracts_direct_message_text() {
        let update = parse_update(
            r#"
            {
              "message": {
                "chat": {
                  "first_name": "Hirrolot",
                  "id": 408258968,
                  "type": "private",
                  "username": "hirrolot"
                },
                "date": 1581448857,
                "from": {
                  "first_name": "Hirrolot",
                  "id": 408258968,
                  "is_bot": false,
                  "language_code": "en",
                  "username": "hirrolot"
                },
                "message_id": 154,
                "text": "4"
              },
              "update_id": 306197398
            }
            "#,
        );

        let incoming = extract_text_message(&update).expect("message extracted");
        assert_eq!(incoming.envelope.chat_kind, TelegramChatKind::Direct);
        assert!(!incoming.envelope.is_forum);
        assert_eq!(incoming.envelope.message_thread_id, None);
        assert_eq!(incoming.text, "4");
    }

    #[test]
    fn ignores_non_message_updates() {
        let update = parse_update(
            r#"{
                "update_id": 3,
                "callback_query": {
                    "id": "abc",
                    "from": {
                        "id": 1668851955,
                        "is_bot": false,
                        "first_name": "Baha"
                    },
                    "chat_instance": "inst",
                    "data": "noop"
                }
            }"#,
        );

        assert!(extract_text_message(&update).is_none());
    }
}
