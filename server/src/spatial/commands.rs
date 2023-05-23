use bevy::prelude::*;
use bevy_nest::prelude::*;
use regex::Regex;

use crate::{
    player::{
        components::{Character, Client},
        permissions,
    },
    visual::components::Sprite,
    world::resources::TileMap,
};

use super::{
    components::{Impassable, Position, Tile, Transition, Zone},
    utils::{offset_for_direction, view_for_tile},
};

// USAGE: (look|l)
pub(super) fn look(
    tile_map: Res<TileMap>,
    mut inbox: EventReader<Inbox>,
    mut outbox: EventWriter<Outbox>,
    players: Query<(&Client, &Position), With<Character>>,
    tiles: Query<(&Tile, &Sprite)>,
) {
    let regex = Regex::new(r"^(look|l)$").unwrap();

    for (message, _) in inbox.iter().filter_map(|message| match &message.content {
        Message::Text(text) if regex.is_match(text) => Some((message, text)),
        _ => None,
    }) {
        let Some((client, player_position)) = players.iter().find(|(c, _)| c.0 == message.from) else {
            return;
        };

        let Some((tile, sprite)) = tile_map
                .get(player_position.zone, player_position.coords)
                .and_then(|e| tiles.get(*e).ok()) else {
                    return;
                };

        outbox.send_text(client.0, view_for_tile(tile, sprite));
    }
}

// USAGE: (map|m)
pub(super) fn map(
    tile_map: Res<TileMap>,
    mut inbox: EventReader<Inbox>,
    mut outbox: EventWriter<Outbox>,
    players: Query<(&Client, &Position), With<Character>>,
    tiles: Query<&Sprite, With<Tile>>,
) {
    let regex = Regex::new(r"^(map|m)$").unwrap();

    for (message, _) in inbox.iter().filter_map(|message| match &message.content {
        Message::Text(text) if regex.is_match(text) => Some((message, text)),
        _ => None,
    }) {
        let Some((client, player_position)) = players.iter().find(|(c, _)| c.0 == message.from) else {
            return;
        };

        let width = 64;
        let height = 16;

        let mut map = vec![vec![' '; width]; height];

        let start_x = player_position.coords.x - (width as i32 / 2);
        let end_x = player_position.coords.x + (width as i32 / 2);
        let start_y = player_position.coords.y - (height as i32 / 2);
        let end_y = player_position.coords.y + (height as i32 / 2);

        for x in start_x..=end_x {
            for y in start_y..=end_y {
                if x == player_position.coords.x && y == player_position.coords.y {
                    map[(y - start_y) as usize][(x - start_x) as usize] = '@';
                } else if let Some(sprite) = tile_map
                    .get(
                        player_position.zone,
                        IVec3::new(x, y, player_position.coords.z),
                    )
                    .and_then(|e| tiles.get(*e).ok())
                {
                    map[(y - start_y) as usize][(x - start_x) as usize] =
                        sprite.character.chars().next().unwrap_or(' ');
                }
            }
        }

        let display = map
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        outbox.send_text(client.0, format!("{}\n{}", player_position.zone, display));
    }
}

// USAGE: <direction>
pub(super) fn movement(
    tile_map: Res<TileMap>,
    mut inbox: EventReader<Inbox>,
    mut outbox: EventWriter<Outbox>,
    mut players: Query<(&Client, &mut Position), With<Character>>,
    tiles: Query<(&Position, &Tile, &Sprite, Option<&Impassable>), Without<Character>>,
) {
    let regex = Regex::new(
        r"^(north|n|northeast|ne|east|e|southeast|se|south|s|southwest|sw|west|w|northwest|nw|up|u|down|d)$",
    )
    .unwrap();

    for (message, direction) in inbox.iter().filter_map(|message| match &message.content {
        Message::Text(text) if regex.is_match(text) => Some((message, text)),
        _ => None,
    }) {
        let Some((client, mut player_position)) = players.iter_mut().find(|(c, _)| c.0 == message.from) else {
            return;
        };

        let Some(offset) = offset_for_direction(direction) else {
            return;
        };

        let Some((tile_position, tile, sprite, impassable)) = tile_map
                .get(player_position.zone, player_position.coords + offset)
                .and_then(|e| tiles.get(*e).ok()) else {
                    return;
                };

        if impassable.is_none() {
            player_position.coords = tile_position.coords;

            outbox.send_text(client.0, view_for_tile(tile, sprite))
        } else {
            outbox.send_text(client.0, "Something blocks your path.");
        }
    }
}

// USAGE: (enter) [target]
pub(super) fn enter(
    mut inbox: EventReader<Inbox>,
    mut outbox: EventWriter<Outbox>,
    mut players: Query<(&Client, &mut Position)>,
    transitions: Query<(&Position, &Transition), Without<Client>>,
    tiles: Query<(&Position, &Tile, &Sprite), Without<Client>>,
) {
    let regex = Regex::new(r"^(enter)( .+)?$").unwrap();

    for (message, captures) in inbox.iter().filter_map(|message| match &message.content {
        Message::Text(text) => regex.captures(text).map(|caps| (message, caps)),
        _ => None,
    }) {
        let Some((client, mut player_position)) = players.iter_mut().find(|(c, _)| c.0 == message.from) else {
            return;
        };

        let target = captures.get(2).map(|m| m.as_str());

        let transition = transitions
            .iter()
            .filter(|(p, _)| p.zone == player_position.zone)
            .find(|(p, t)| {
                p.coords == player_position.coords
                    && target
                        .as_ref()
                        .map_or(true, |tag| t.tags.contains(&tag.trim().to_string()))
            });

        if let Some((_, transition)) = transition {
            player_position.zone = transition.zone;
            player_position.coords = transition.coords;

            if let Some((_, tile, sprite)) = tiles
                .iter()
                .filter(|(p, _, _)| p.zone == player_position.zone)
                .find(|(p, _, _)| p.coords == player_position.coords)
            {
                outbox.send_text(client.0, view_for_tile(tile, sprite))
            }
        }
    }
}

// USAGE: (teleport|tp) (here|<zone>) (<x> <y> <z>)
pub(super) fn teleport(
    mut inbox: EventReader<Inbox>,
    mut players: Query<(&Client, &mut Position, &Character)>,
) {
    let regex = Regex::new(r"^(teleport|tp) (here|(.+)) \(((\d) (\d) (\d))\)$").unwrap();

    for (message, captures) in inbox.iter().filter_map(|message| match &message.content {
        Message::Text(text) => regex.captures(text).map(|caps| (message, caps)),
        _ => None,
    }) {
        let Some((_, mut player_position, character)) = players.iter_mut().find(|(c, _, _)| c.0 == message.from) else {
            return;
        };

        if !character.can(permissions::TELEPORT) {
            return;
        }

        let region = captures.get(2).map(|m| m.as_str()).unwrap_or("here");
        let x = captures
            .get(5)
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or_default();
        let y = captures
            .get(6)
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or_default();
        let z = captures
            .get(7)
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or_default();

        info!(
            "Teleporting {} to ({}, {}, {}) in {}",
            character.name, x, y, z, region
        );

        if region != "here" {
            player_position.zone = match region {
                "movement" => Zone::Movement,
                "void" => Zone::Void,
                _ => Zone::Void,
            }
        }

        player_position.coords = IVec3::new(x, y, z);
    }
}
