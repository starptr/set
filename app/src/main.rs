#![warn(clippy::str_to_string)]

mod commands;

use poise::serenity_prelude as serenity;
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::{Arc, atomic},
    time::Duration,
    fs,
    path,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// Types used by all command functions
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Serialize, Deserialize)]
struct MessagesCache {
    cache: HashSet<String>,
    last_message_id: Option<serenity::MessageId>,
}
impl MessagesCache {
    fn new() -> Self {
        Self {
            cache: HashSet::new(),
            last_message_id: None,
        }
    }
    fn from_file(data_file: fs::File) -> Self {
        // TODO: refactor
        let data: MessagesCache = serde_json::from_reader(data_file).expect("Failed to deserialize data file");
        data
    }
    fn to_file(data_file: fs::File) -> Self {
        // TODO: refactor
        unimplemented!("Implement saving to file")
    }
}

// Custom user data passed to all command functions
pub struct Data {
    messages_cache: Arc<Mutex<MessagesCache>>,
    //votes: Mutex<HashMap<String, u32>>,
    uncommitted_count: atomic::AtomicU32,
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

fn get_the_channel_id() -> u64 {
    env::var("CHANNEL_ID")
        .expect("Missing `CHANNEL_ID` env var. Set it to the channel ID to listen to.")
        .parse()
        .expect(format!("Failed to convert `CHANNEL_ID` {} to a u64", env::var("CHANNEL_ID").unwrap()).as_str())
}

fn get_the_data_path() -> path::PathBuf {
    let cwd = env::current_dir().expect("Failed to get current directory");
    let data_path = cwd.join("set-bot-cache.json");
    data_path
}

fn noramlize_string(msg: &str) -> String {
    let msg = msg.to_lowercase();
    use unicode_normalization::UnicodeNormalization;
    // Apply Unicode Normalization Form C
    let msg: String = msg.nfc().collect();
    // Remove all whitespaces, and split into tokens (formerly separated by whitespaces)
    let tokens: Vec<_> = msg.split_whitespace().collect();
    tokens.join(" ")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    dotenvy::dotenv().expect("Failed to load .env file");

    let _ = get_the_channel_id();

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![commands::help(), commands::check()],
        prefix_options: poise::PrefixFrameworkOptions {
            edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(
                Duration::from_secs(3600),
            ))),
            mention_as_prefix: true,
            ..Default::default()
        },
        // The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        // This code is run before every command
        pre_command: |ctx| {
            Box::pin(async move {
                println!("Executing command {}...", ctx.command().qualified_name);
            })
        },
        // This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                println!("Executed command {}!", ctx.command().qualified_name);
            })
        },
        // Every command invocation must pass this check to continue execution
        command_check: Some(|ctx| {
            Box::pin(async move {
                if ctx.author().id == 123456789 {
                    return Ok(false);
                }
                Ok(true)
            })
        }),
        // Enforce command checks even for owners (enforced by default)
        // Set to true to bypass checks, which is useful for testing
        skip_checks_for_owners: false,
        event_handler: |ctx, event, _framework, data| {
            Box::pin(async move {
                match event {
                    serenity::FullEvent::Ready{data_about_bot: _} => {
                        let channel_id = get_the_channel_id();
                        let channel = serenity::ChannelId::new(channel_id).to_channel(ctx).await;
                        let channel = match channel {
                            Ok(serenity::Channel::Guild(channel)) => channel,
                            _ => {
                                println!("Channel is of the wrong type");
                                return Err("Channel is of the wrong type".into());
                            }
                        };
                        {
                            let mut messages_cache = data.messages_cache.lock().await;
                            let mut last_message_id = messages_cache.last_message_id;
                            loop {
                                let query = match last_message_id {
                                    Some(last_message_id) => serenity::builder::GetMessages::new()
                                        .after(last_message_id),
                                    None => serenity::builder::GetMessages::new().limit(100), // INFO: this is technically bugged, since without any specification, messages are ordered by most recent
                                };
                                let msgs = channel.messages(ctx, query).await?;
                                if msgs.is_empty() {
                                    break;
                                }
                                for message in &msgs {
                                    let msg = noramlize_string(&message.content);
                                    println!("Catching up on msg from {:?}: {}", message.author_nick(ctx).await, msg);
                                    let newly_inserted = messages_cache.cache.insert(msg);
                                    if !newly_inserted {
                                        println!("Deleting duplicate message");
                                        let res = message.delete(ctx).await;
                                        if let Err(error) = res {
                                            println!("Failed to delete message: {:?}", error);
                                        }
                                    }
                                }
                                last_message_id = Some(msgs.first().unwrap().id); // messages are returned in reverse order (bottom to top)
                            }
                            messages_cache.last_message_id = last_message_id;
                        }
                        println!("Committing messages to disk");
                        {
                            let messages_cache = data.messages_cache.lock().await;
                            let file = get_the_data_path();
                            let file = fs::File::create(file)?;
                            serde_json::to_writer_pretty(&file, &*messages_cache)?;
                        }
                        Ok(())
                    }
                    serenity::FullEvent::Message{new_message} => {
                        if new_message.channel_id != get_the_channel_id() {
                            println!("Got an event {:?} for channel {:?}, ignoring", event.snake_case_name(), new_message.channel_id);
                            return Ok(());
                        }
                        println!("Handling message from {:?}: {}", new_message.author_nick(ctx).await, new_message.content);
                        let msg = noramlize_string(&new_message.content);
                        let newly_inserted = {
                            let mut messages_cache = data.messages_cache.lock().await;
                            messages_cache.last_message_id = Some(new_message.id);
                            messages_cache.cache.insert(msg)
                        };
                        if !newly_inserted {
                            print!("Deleting duplicate message");
                            let res = new_message.delete(ctx).await;
                            if let Err(error) = res {
                                println!("Failed to delete message: {:?}", error);
                            }
                        }
                        //let ct = data.uncommitted_count.fetch_add(1, atomic::Ordering::SeqCst);
                        //if ct >= 9 {
                        println!("Committing messages to disk");
                        {
                            let messages_cache = data.messages_cache.lock().await;
                            let file = get_the_data_path();
                            let file = fs::File::create(file)?;
                            serde_json::to_writer_pretty(&file, &*messages_cache)?;
                        }
                        //    data.uncommitted_count.store(0, atomic::Ordering::SeqCst);
                        //}
                        Ok(())
                    }
                    _ => {
                        println!("Got an event: {:?}", event.snake_case_name());
                        Ok(())
                    }
                }
            })
        },
        ..Default::default()
    };

    let file = get_the_data_path();
    let file = fs::File::open(file);
    let messages_cache = match file {
        Ok(file) => MessagesCache::from_file(file),
        Err(_) => MessagesCache::new(),
    };

    let framework = poise::Framework::builder()
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", _ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    messages_cache: Arc::new(Mutex::new(messages_cache)),
                    //votes: Mutex::new(HashMap::new()),
                    uncommitted_count: atomic::AtomicU32::new(0),
                })
            })
        })
        .options(options)
        .build();

    let token = env::var("DISCORD_TOKEN")
        .expect("Missing `DISCORD_TOKEN` env var, see README for more information.");
    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}