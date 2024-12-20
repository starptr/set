use crate::{Context, Error, get_the_channel_id};
use poise::serenity_prelude::{self as serenity, all, model::permissions};

/// Show this help menu
#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            extra_text_at_bottom: "This is an example bot made to showcase features of my custom Discord bot framework",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn check(
    ctx: Context<'_>,
    #[description = "Check required perms"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    _command: Option<String>,
) -> Result<(), Error> {
    let channel_id = serenity::ChannelId::new(get_the_channel_id());
    let channel = match channel_id.to_channel(&ctx).await {
        Ok(serenity::Channel::Guild(channel)) => channel,
        Ok(serenity::Channel::Private(_channel)) => {
            ctx.say(format!("Channel {} is a private channel", channel_id)).await?;
            return Ok(());
        },
        Ok(_) => {
            ctx.say(format!("Channel {} is not a guild channel", channel_id)).await?;
            return Ok(());
        },
        Err(e) => {
            ctx.say(format!("Failed to get channel with ID {}: {}", channel_id, e)).await?;
            return Ok(());
        },
    };
    let bot_user = ctx.http().get_current_user().await?;
    let permissions = channel.permissions_for_user(&ctx, bot_user.id)?;

    let mut all_correct = true;
    if !permissions.contains(serenity::Permissions::MANAGE_MESSAGES) {
        all_correct = false;
        ctx.say(format!("Bot user does not have the MANAGE_MESSAGES permission for Channel {}", channel_id)).await?;
    }
    if !permissions.contains(serenity::Permissions::READ_MESSAGE_HISTORY) {
        all_correct = false;
        ctx.say(format!("Bot user does not have the READ_MESSAGE_HISTORY permission for Channel {}", channel_id)).await?;
    }
    if all_correct {
        ctx.say("No incorrect settings for bot user were detected.").await?;
    }
    Ok(())
}


///// Vote for something
/////
///// Enter `~vote pumpkin` to vote for pumpkins
//#[poise::command(prefix_command, slash_command)]
//pub async fn vote(
//    ctx: Context<'_>,
//    #[description = "What to vote for"] choice: String,
//) -> Result<(), Error> {
//    // Lock the Mutex in a block {} so the Mutex isn't locked across an await point
//    let num_votes = {
//        let mut hash_map = ctx.data().votes.lock().unwrap();
//        let num_votes = hash_map.entry(choice.clone()).or_default();
//        *num_votes += 1;
//        *num_votes
//    };
//
//    let response = format!("Successfully voted for {choice}. {choice} now has {num_votes} votes!");
//    ctx.say(response).await?;
//    Ok(())
//}
//
///// Retrieve number of votes
/////
///// Retrieve the number of votes either in general, or for a specific choice:
///// ```
///// ~getvotes
///// ~getvotes pumpkin
///// ```
//#[poise::command(prefix_command, track_edits, aliases("votes"), slash_command)]
//pub async fn getvotes(
//    ctx: Context<'_>,
//    #[description = "Choice to retrieve votes for"] choice: Option<String>,
//) -> Result<(), Error> {
//    if let Some(choice) = choice {
//        let num_votes = *ctx.data().votes.lock().unwrap().get(&choice).unwrap_or(&0);
//        let response = match num_votes {
//            0 => format!("Nobody has voted for {} yet", choice),
//            _ => format!("{} people have voted for {}", num_votes, choice),
//        };
//        ctx.say(response).await?;
//    } else {
//        let mut response = String::new();
//        for (choice, num_votes) in ctx.data().votes.lock().unwrap().iter() {
//            response += &format!("{}: {} votes", choice, num_votes);
//        }
//
//        if response.is_empty() {
//            response += "Nobody has voted for anything yet :(";
//        }
//
//        ctx.say(response).await?;
//    };
//
//    Ok(())
//}