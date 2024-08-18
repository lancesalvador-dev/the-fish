#![allow(unused_imports)]
use std::alloc::System;
use std::env;
use std::fmt::Debug;
use std::hash::BuildHasher;
use std::io::Bytes;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};

use serenity::async_trait;
use serenity::prelude::*;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::Timestamp;
use serenity::utils::MessageBuilder;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{
    help_commands, Args, 
    CommandGroup, CommandOptions, CommandResult,
    DispatchError, HelpOptions, Reason, StandardFramework,
};
use serenity::utils::Colour;
use reqwest::Url;
use rosu_pp::{
    Beatmap, OsuPP, BeatmapExt, 
    DifficultyAttributes, PerformanceAttributes
};
use rosu::{model::*, Osu, OsuResult, request};
use rgb::RGB8 as Color;
use color_thief::get_palette;
use std::io::Cursor;
use image::io::Reader as ImageReader;

struct Handler;



#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, context: Context, msg: Message) {
        // !embed example command
        if msg.content == "!embed" {
            let msg = msg
            .channel_id
            .send_message(&context.http, |m| {
                m.content("This is an embed:")
                    .embed(|e| {
                        e.title("This is the title of the embed")
                            .color(Colour::DARK_BLUE)
                            .description("This is the description")
                            .fields(vec![
                                ("This is the first field", "First field's body", true),
                                ("This is the second field", "Both fields are inline", true),
                            ])
                            .field("This is the third field", "this is not an inline field", true)
                            .image("https://cdn.discordapp.com/attachments/375460864150470678/1132897955540635649/choose_your_main.jpg")
                            .footer(|f| f.text("this embed is fish certified™"))
                            .timestamp(Timestamp::now())
                    }   )
                    // .add_file("./cover.jpg")
            })
            .await;
            if let Err(why) = msg {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is in", ready.user.name);
    }
}


#[tokio::main]
async fn main() {
    dotenv::dotenv().ok(); // loads the tokens from the .env file
    let token = env::var("DISCORD_TOKEN").expect("Fatality! DISCORD_TOKEN not set!");
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~")) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP);
    
    let intents = GatewayIntents::non_privileged()
    | GatewayIntents::GUILD_MESSAGES
    | GatewayIntents::DIRECT_MESSAGES
    | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
        println!("Did you set your tokens in '.env'?")
    }
}

#[group]
#[commands(help, hi, ping, pp, get_ids)]
struct General;

#[command]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let commands = "
```yaml
~help # Sends the message you're currently reading.
~hi # Say hi to the fish.
~ping # Prints the latency between the user's message and the fish's response.
~pp [beatmap link] # Performs pp calculation on a given beatmap link.
```";
    msg.reply(ctx, commands).await?;
    Ok(())
}

#[command]
async fn hi(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Hi, I'm a fish, and definitely not a robot. Definitely.").await?;
    Ok(())
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    let user_msg_time = msg.timestamp.timestamp_millis();
    if let Err(why) = msg.reply(ctx, format!("fish. ({}ms)", 
    (Timestamp::now().timestamp_millis() - user_msg_time))).await {
        println!("Error sending message: {why:?}");
    };
    
    Ok(())
}

#[command]
async fn pp(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let link = args.single::<String>()?; // get string (link) from message 
    let ids = return_ids(&link); // get ids from link
    if ids.is_err(){
        msg.reply(ctx, "IDs returned invalid.").await?;
        // can't figure out how to abort command here
    }

    let (mapset_id, beatmap_id) = ids?; // get ids from link

    // putting the entire image in memory just to get the primary color out of it lol 
    let image_bytes = reqwest::get(format!("https://assets.ppy.sh/beatmaps/{}/covers/cover.jpg", mapset_id)).await?.bytes().await?;
    let image = ImageReader::new(Cursor::new(image_bytes)).with_guessed_format()?.decode()?;
    let palette: Vec<Color>;
    let (pixels, color_format) = get_image_buffer(&image)?;
    palette = get_palette(&pixels, color_format, 1, 2).unwrap_or(vec![Color{r:0, g:0, b:0}]);

    let map_bytes = reqwest::get(format!("https://osu.ppy.sh/osu/{}", beatmap_id)).await?.bytes().await?;
    let map = Beatmap::from_bytes(&map_bytes)?; // makes rosu_pp::Beatmap
    let pp_ratings = one_whole_pp(&map);
    let osu = Osu::new(env::var("OSU_API_KEY").expect("Fatality! OSU_API_KEY not set!")); // osu api key
    let map = osu.beatmap().map_id(beatmap_id).await?.ok_or(anyhow!("Error trying to create Beatmap from map ID"))?; // makes rosu::Beatmap (for artist, songtitle, diffname (map.version)
    let msg = msg
            .channel_id
            .send_message(&ctx.http, |m| {
                m.content("")
                    .embed(|e| {
                        e.title(format!("{} - {}\n[{}]", map.artist, map.title, map.version))
                        .color(Colour::from_rgb(palette[0].r, palette[0].g, palette[0].b)) 
                        .description(pp_ratings)
                        .fields(vec![
                            ("AR", &map.diff_ar, true),
                            ("OD", &map.diff_od, true),
                            ("CS", &map.diff_cs, true), 
                            ("HP", &map.diff_hp, false)
                        ])
                        .image(format!("https://assets.ppy.sh/beatmaps/{}/covers/cover.jpg", mapset_id))
                        .footer(|f| f.text("this embed is fish certified™"))
                        .timestamp(Timestamp::now())
                    }   )
            })
            .await;

            if let Err(why) = msg {
                println!("Error sending message: {:?}", why);
            }

    Ok(())
}

#[command]
async fn get_ids(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let link = args.single::<String>().unwrap();

    let ids = return_ids(&link);
    if ids.is_err(){
        msg.reply(ctx, "could not get ids").await?;
    }
    let (mapset_id, beatmap_id) = ids?;
    let fishspeak = format!("mapset id: {}\nbeatmap id: {}", mapset_id, beatmap_id);
    msg.reply(ctx, fishspeak).await?;
    let osu = Osu::new("e6b9b03cbb7475fa58b97dfcb8fdff3624d96838"); // **********improtant I think*******************  osu api key
    let getmap = osu.beatmap().map_id(beatmap_id).await?;
    let map = getmap.unwrap();
    let name = format!("{} - {}\n[{}]", map.artist, map.title, map.version);
    msg.reply(ctx, name).await?;

    Ok(())
}


// pp calculation - finds pp for 95% and 100%
fn one_whole_pp(bmap: &Beatmap) -> String {
        let p_attributes = bmap.max_pp(0);
        let max_crombo = p_attributes.max_combo();
        let fc_95 = OsuPP::new(&bmap)
        .combo(max_crombo) 
        .accuracy(95.0)
        .n_misses(0)
        .calculate();
        let pp_95 = &fc_95.pp().round().to_string();
        let pp_max = &bmap.max_pp(0).pp().round().to_string();

        let desc = format!("**__assuming nomod fc:__**\n95%: {}pp\n100%: {}pp", pp_95, pp_max);
        desc
}

// 
fn get_image_buffer(img: &image::DynamicImage) -> Result<(Vec<u8>, color_thief::ColorFormat)> {
    if let image::DynamicImage::ImageRgb8(buffer) = img {
        return Ok((buffer.to_vec(), color_thief::ColorFormat::Rgb));
    }
    Err(anyhow!("Error loading image."))
}

fn return_ids(link: &String) -> Result<(u32, u32)> {
    let theline = Url::parse(link)?;
    let mut path_segments = theline.path_segments().ok_or(anyhow!("error getting url path segments"))?;
    let mapset_id = path_segments.nth(1).ok_or(anyhow!("error getting mapset id"))?;
    let fragment = theline.fragment().ok_or(anyhow!("error getting fragment"))?;
    let beatmap_id = fragment.split('/').nth(1).ok_or(anyhow!("error getting beatmap id"))?;
    dbg!(&mapset_id, &beatmap_id);
    Ok((mapset_id.parse::<u32>()?, beatmap_id.parse::<u32>()?))
}
