use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use tracing::{error, info};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client as HttpClient;
use twilight_model::channel::Message;
use twilight_model::gateway::payload::incoming::MessageCreate;
use twilight_model::id::{
    Id,
    marker::{ChannelMarker, MessageMarker},
};

#[derive(Clone, Debug)]
pub struct ChatEvent {
    pub channel_id: u64,
    pub author_id: u64,
    pub author: String,
    pub content: String,
}

/// Starts a Discord Gateway connection.
/// Login as discord bot using `token`.
pub async fn start_gateway(token: String) -> Result<mpsc::Receiver<ChatEvent>> {
    let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT;

    let mut shard = Shard::new(ShardId::ONE, token, intents);

    let (tx, rx) = mpsc::channel::<ChatEvent>(1000);

    tokio::spawn(async move {
        info!("Gateway task started");

        while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
            let event = match item {
                Ok(ev) => ev,
                Err(e) => {
                    error!("Gateway receive error: {e}");
                    continue;
                }
            };

            if let Event::MessageCreate(msg) = event {
                let ev = convert_message_create(*msg);
                let _ = tx.send(ev).await;
            }
        }

        info!("Gateway task ended");
    });

    Ok(rx)
}

pub async fn fetch_messages(token: &str, channel_id: u64, limit: usize) -> Result<Vec<ChatEvent>> {
    let http = HttpClient::new(token.to_string());
    let channel_id: Id<ChannelMarker> = Id::new(channel_id);

    let mut out: Vec<Message> = Vec::new();
    let mut before: Option<Id<MessageMarker>> = None;

    while out.len() < limit {
        let remaining = limit - out.len();
        let batch_size = remaining.min(100) as u16;

        let mut batch: Vec<Message> = if let Some(b) = before {
            http.channel_messages(channel_id)
                .before(b)
                .limit(batch_size)
                .await
                .map_err(|e| anyhow!("Discord HTTP error: {e}"))?
                .model()
                .await
                .map_err(|e| anyhow!("Discord HTTP model error: {e}"))?
        } else {
            http.channel_messages(channel_id)
                .limit(batch_size)
                .await
                .map_err(|e| anyhow!("Discord HTTP error: {e}"))?
                .model()
                .await
                .map_err(|e| anyhow!("Discord HTTP model error: {e}"))?
        };

        if batch.is_empty() {
            break;
        }

        let oldest_id = batch.last().unwrap().id;
        before = Some(oldest_id);

        out.append(&mut batch);
    }

    out.reverse();

    Ok(out
        .into_iter()
        .map(|m| ChatEvent {
            channel_id: m.channel_id.get(),
            author_id: m.author.id.get(),
            author: m.author.name,
            content: m.content,
        })
        .collect())
}

fn convert_message_create(msg: MessageCreate) -> ChatEvent {
    ChatEvent {
        channel_id: msg.channel_id.get(),
        author_id: msg.author.id.get(),
        author: msg.author.name.clone(),
        content: msg.content.clone(),
    }
}

/// Send a message to a channel using the Discord REST API.
pub async fn send_message(token: &str, channel_id: u64, content: &str) -> Result<()> {
    let http = HttpClient::new(token.to_string());

    let channel_id: Id<ChannelMarker> = Id::new(channel_id);

    http.create_message(channel_id)
        .content(content)
        .await
        .map_err(|e| anyhow!("Discord HTTP error: {e}"))?;

    Ok(())
}
