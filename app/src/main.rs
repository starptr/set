#![warn(clippy::str_to_string)]

mod commands;

use poise::serenity_prelude as serenity;
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::{Arc, Mutex},
    time::Duration,
    fs,
};
use serde::{Deserialize, Serialize};

// Types used by all command functions
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Serialize, Deserialize)]
struct MessagesCache {
    cache: HashSet<String>,
    last_appended: serenity::model::timestamp::Timestamp,
}
impl MessagesCache {
    fn new() -> Self {
        Self {
            cache: HashSet::new(),
            last_appended: serenity::model::timestamp::Timestamp::from_millis(0).unwrap(),
        }
    }
    fn from_file(data_file: fs::File) -> Self {
        unimplemented!("Implement loading from file")
    }
}

// Custom user data passed to all command functions
pub struct Data {
    messages_cache: Mutex<MessagesCache>,
    //votes: Mutex<HashMap<String, u32>>,
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

#[tokio::main]
async fn main() {
    env_logger::init();

    let _ = get_the_channel_id();

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![commands::help()],
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
                    serenity::FullEvent::Message{new_message} => {
                        if new_message.channel_id != get_the_channel_id() {
                            println!("Got an event {:?} for channel {:?}, ignoring", event.snake_case_name(), new_message.channel_id);
                            return Ok(());
                        }
                        let msg = {
                            let msg = &new_message.content;
                            use unicode_normalization::UnicodeNormalization;
                            // Apply Unicode Normalization Form C
                            let msg: String = msg.nfc().collect();
                            // Remove all whitespaces, and split into tokens (formerly separated by whitespaces)
                            let tokens: Vec<_> = msg.split_whitespace().collect();
                            tokens.join(" ")
                        };
                        let newly_inserted = {
                            let mut messages_cache = data.messages_cache.lock().unwrap();
                            messages_cache.cache.insert(msg)
                        };
                        if !newly_inserted {
                            let res = new_message.delete(ctx).await;
                            if let Err(error) = res {
                                println!("Failed to delete message: {:?}", error);
                            }
                        }
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

    let framework = poise::Framework::builder()
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", _ready.user.name);
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    messages_cache: Mutex::new(MessagesCache::new()),
                    //votes: Mutex::new(HashMap::new()),
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