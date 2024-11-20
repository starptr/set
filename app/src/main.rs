use std::env;
use std::sync::Arc;
use std::time::Duration;
use chrono::DateTime;
use chrono::Utc;
use dotenvy::dotenv;

use serenity::all::ChannelId;
use serenity::async_trait;
use serenity::builder::GetMessages;
use serenity::http;
use serenity::prelude::*;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::timestamp::Timestamp;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{StandardFramework, Configuration, CommandResult};
use tokio::task::JoinHandle;
use tokio::time::sleep;

fn get_link_to_msg(msg: &Message) -> Option<String> {
    if msg.guild_id.is_some() {
        Some(format!("https://discord.com/channels/{}/{}/{}", msg.guild_id.unwrap().get(), msg.channel_id.get(), msg.id.get()))
    } else {
        None
    }
}

fn get_fleeting_channel_id() -> ChannelId {
    use std::str::FromStr;
    let channel_id = env::var("CHANNEL_ID").expect("Channel ID not specified");
    ChannelId::from_str(&channel_id).unwrap()
}

#[group]
#[commands(ping)]
struct General;

fn serenity_ts_to_chrono_dt(ts: Timestamp) -> DateTime<Utc> {
    let ts = ts.to_rfc3339().unwrap();
    DateTime::parse_from_rfc3339(&ts).unwrap().with_timezone(&Utc)
}

#[derive(Clone)]
struct Handler{
    messages: Arc<Mutex<Vec<Message>>>,
    timer: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Handler {
    fn new(messages: Arc<Mutex<Vec<Message>>>) -> Self {
        Self { messages, timer: Arc::new(Mutex::new(None)) }
    }

    /**
     * Start the timer to process messages
     * The timer will not stop until there are no more messages to process
     */
    async fn start_timer(&self, ctx: Context) {
        let messages = self.messages.clone();
        let timer = self.timer.clone();
        let http = ctx.http.clone();
        {
            let mut timer_shared_state = timer.lock().await;
            // Early exit if the timer is already running
            if timer_shared_state.is_some() { return; }


            let timer = self.timer.clone();
            let task = tokio::spawn(async move {
                let mut timer_shared_state = timer.lock().await;
                loop {
                    let message = {
                        let mut messages = messages.lock().await;
                        // Early exit if there are no messages to process
                        if messages.is_empty() {
                            *timer_shared_state = None;
                            break;
                        }
                        messages.remove(0)
                    };
                    // Compute sleep duration
                    let day = std::time::Duration::from_secs(60);
                    let sleep_duration = {
                        let timestamp = message.timestamp;
                        let target: DateTime<Utc> = serenity_ts_to_chrono_dt(timestamp) + day;
                        let duration = target.signed_duration_since(Utc::now());
                        duration
                    };
                    // Sleep until the message is ready to be processed
                    sleep(sleep_duration.to_std().unwrap_or(Duration::ZERO)).await;

                    // Process message
                    let threshold = {
                        let threshold = std::time::SystemTime::now() - day;
                        let threshold = threshold.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap().as_millis();
                        Timestamp::from_millis(threshold.try_into().unwrap()).unwrap()
                    };
                    if message.timestamp <= threshold {
                        // Delete message
                        match message.delete(&http).await {
                            Ok(_) => {},
                            Err(e) => {
                                let link_str = get_link_to_msg(&message).map_or("".to_string(), |link| link);
                                eprintln!("Error deleting message {link_str}: {}", e);
                                // Re-insert message into queue
                                let messages = messages.clone();
                                {
                                    let mut messages = messages.lock().await;
                                    messages.insert(0, message);
                                }
                            }
                        }
                        //println!("Del: {}", get_link_to_msg(&message).unwrap_or(message.content.to_string()));
                    } else {
                        panic!("Message was not ready to be processed but should have been ready");
                    }
                }
            });
            *timer_shared_state = Some(task);
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    // Add new messages to the all-messages vector
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.channel_id != get_fleeting_channel_id() { return; }
        {
            let mut messages = self.messages.lock().await;
            messages.push(msg);
        }
        // Start the timer to process them
        let this = Arc::new(Mutex::new(self.clone()));
        tokio::spawn(async move {
            let this = this.clone();
            let handler = this.lock().await;
            handler.start_timer(ctx).await;
        });
    }

    // Populate the messages vector with all messages in the channel
    async fn ready(&self, ctx: Context, _ready: Ready) {
        // Get channelid obj
        let fleeting_channelid = get_fleeting_channel_id();
        // Populate all messages
        let mut last_id = None;
        loop {
            let messages_slice = fleeting_channelid.messages(&ctx.http, {
                let retriever = GetMessages::new()
                    .limit(100);
                let retriever = if let Some(id) = last_id {
                    // Continue where we left off from the last slice
                    retriever.before(id)
                } else { retriever };
                retriever
            }).await.expect("Error fetching messages");

            if messages_slice.is_empty() {
                // No more messages
                break;
            }
            last_id = messages_slice.last().map(|m| m.id);

            // Continue populating all messages with more messages
            {
                let mut messages = self.messages.lock().await;
                for message in messages_slice {
                    messages.push(message);
                }
            }
        }
        // Flip order because newest was at the beginning
        {
            let mut messages = self.messages.lock().await;
            messages.reverse();
        }

        // Start the timer to process them
        let this = Arc::new(Mutex::new(self.clone()));
        tokio::spawn(async move {
            let this = this.clone();
            let handler = this.lock().await;
            handler.start_timer(ctx).await;
        });
    }
}

#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");

    let framework = StandardFramework::new().group(&GENERAL_GROUP);
    framework.configure(Configuration::new().prefix("。")); // set the bot's prefix to "。"

    // Queue of messages
    let messages = Arc::new(Mutex::new(Vec::new()));

    // Login with a bot token from the environment
    let token = env::var("DISCORD_TOKEN").expect("token");
    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_MESSAGES;
    let mut client = Client::builder(token, intents)
        .event_handler(Handler::new(messages.clone()))
        .framework(framework)
        .await
        .expect("Error creating client");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;

    println!("Msg: {}", get_link_to_msg(msg).unwrap_or("No link".to_string()));

    Ok(())
}