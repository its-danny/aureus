use std::sync::OnceLock;

use bevy::prelude::*;
use bevy_nest::prelude::*;
use regex::Regex;

use crate::{
    input::events::{Command, ParsedCommand},
    player::components::{Character, Client},
    spatial::{
        components::{Position, Tile, Transition},
        utils::view_for_tile,
    },
    visual::components::Sprite,
};

static REGEX: OnceLock<Regex> = OnceLock::new();

pub fn parse_enter(
    client: &Client,
    content: &str,
    commands: &mut EventWriter<ParsedCommand>,
) -> bool {
    let regex = REGEX.get_or_init(|| Regex::new(r"^(enter)(?P<transition> .+)?$").unwrap());

    if let Some(captures) = regex.captures(content) {
        let target = captures.name("transition").map(|m| m.as_str().trim().to_string());

        commands.send(ParsedCommand {
            from: client.id,
            command: Command::Enter(target),
        });

        true
    } else {
        false
    }
}

pub fn enter(
    mut commands: EventReader<ParsedCommand>,
    mut outbox: EventWriter<Outbox>,
    mut players: Query<(&Client, &mut Position), With<Character>>,
    transitions: Query<&Transition, Without<Client>>,
    tiles: Query<(&Position, &Tile, &Sprite, Option<&Children>), Without<Client>>,
) {
    for command in commands.iter() {
        if let Command::Enter(target) = &command.command {
            let Some((client, mut player_position)) = players.iter_mut().find(|(c, _)| c.id == command.from) else {
                debug!("Could not find player for client: {:?}", command.from);

                continue;
            };

            let Some((_, _, _, siblings)) = tiles.iter().find(|(p, _, _, _)| {
                p.zone == player_position.zone && p.coords == player_position.coords
            }) else {
                debug!("Could not find tile for player position: {:?}", player_position);

                continue;
            };

            let transitions = siblings.map(|siblings| {
                siblings.iter().filter_map(|child| transitions.get(*child).ok()).collect::<Vec<_>>()
            }).unwrap_or_else(|| vec![]);

            if transitions.is_empty() {
                outbox.send_text(client.id, "There is nowhere to enter from here.");
                
                continue;
            }

            let Some(transition) = transitions.iter().find(|transition| {
                target
                    .as_ref()
                    .map_or(true, |tag| transition.tags.contains(tag))
            }) else {
                outbox.send_text(client.id, "Could not find entrance.");

                continue;
            };

            let Some((position, tile, sprite, _)) = tiles.iter().find(|(p, _, _, _)| {
                p.zone == transition.zone && p.coords == transition.coords
            }) else {
                debug!("Could not find tile for transition: {:?}", transition);
                
                continue;
            };

            player_position.zone = position.zone;
            player_position.coords = position.coords;

            outbox.send_text(client.id, view_for_tile(tile, sprite, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        spatial::components::Zone,
        test::{
            app_builder::AppBuilder,
            player_builder::PlayerBuilder,
            tile_builder::TileBuilder,
            transition_builder::TransitionBuilder,
            utils::{get_message_content, send_message},
        },
    };

    use super::*;

    #[test]
    fn enters_by_tag() {
        let mut app = AppBuilder::new().build();
        app.add_system(enter);

        let start = TileBuilder::new().zone(Zone::Void).build(&mut app);
        let destination = TileBuilder::new().zone(Zone::Movement).build(&mut app);

        TransitionBuilder::new(start, destination)
            .tags(&vec!["movement"])
            .build(&mut app);

        let (client_id, player) = PlayerBuilder::new().zone(Zone::Void).build(&mut app);

        send_message(&mut app, client_id, "enter movement");
        app.update();

        let updated_position = app.world.get::<Position>(player).unwrap();

        assert_eq!(updated_position.zone, Zone::Movement);
        assert_eq!(updated_position.coords, IVec3::ZERO);
    }

    #[test]
    fn enters_first_if_no_tag() {
        let mut app = AppBuilder::new().build();
        app.add_system(enter);

        let start = TileBuilder::new().zone(Zone::Void).build(&mut app);
        let destination = TileBuilder::new().zone(Zone::Movement).build(&mut app);
        let nope = TileBuilder::new().zone(Zone::Movement).coords(IVec3::new(1, 1, 1)).build(&mut app);

        TransitionBuilder::new(start, destination).build(&mut app);
        TransitionBuilder::new(start, nope).build(&mut app);

        let (client_id, player) = PlayerBuilder::new().zone(Zone::Void).build(&mut app);

        send_message(&mut app, client_id, "enter");
        app.update();

        let updated_position = app.world.get::<Position>(player).unwrap();

        assert_eq!(updated_position.zone, Zone::Movement);
        assert_eq!(updated_position.coords, IVec3::ZERO);
    }

    #[test]
    fn transition_not_found() {
        let mut app = AppBuilder::new().build();
        app.add_system(enter);

        let start = TileBuilder::new().zone(Zone::Void).build(&mut app);
        let destination = TileBuilder::new().zone(Zone::Movement).build(&mut app);

        TransitionBuilder::new(start, destination)
            .tags(&vec!["movement"])
            .build(&mut app);

        let (client_id, _) = PlayerBuilder::new().zone(Zone::Void).build(&mut app);

        send_message(&mut app, client_id, "enter at your own risk");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert!(content.contains("Could not find entrance."));
    }

    #[test]
    fn no_transition() {
        let mut app = AppBuilder::new().build();
        app.add_system(enter);

        TileBuilder::new().build(&mut app);

        let (client_id, _) = PlayerBuilder::new().build(&mut app);

        send_message(&mut app, client_id, "enter the dragon");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert!(content.contains("There is nowhere to enter from here."));
    }
}
