use std::sync::OnceLock;

use bevy::prelude::*;
use bevy_nest::prelude::*;
use regex::Regex;

use crate::{
    input::events::{Command, ParsedCommand},
    items::{
        components::{CanTake, Inventory, Item},
        utils::item_name_list,
    },
    player::components::{Client, Online},
    spatial::components::Tile,
};

static REGEX: OnceLock<Regex> = OnceLock::new();

pub fn handle_take(
    client: &Client,
    content: &str,
    commands: &mut EventWriter<ParsedCommand>,
) -> bool {
    let regex =
        REGEX.get_or_init(|| Regex::new(r"^(take|get) ((?P<all>all) )?(?P<target>.+)$").unwrap());

    if let Some(captures) = regex.captures(content) {
        let target = captures
            .name("target")
            .map(|m| m.as_str().trim().to_lowercase())
            .unwrap_or_default();

        let all = captures.name("all").is_some();

        commands.send(ParsedCommand {
            from: client.id,
            command: Command::Take((target, all)),
        });

        true
    } else {
        false
    }
}

pub fn take(
    mut bevy: Commands,
    mut commands: EventReader<ParsedCommand>,
    mut outbox: EventWriter<Outbox>,
    mut players: Query<(&Client, &Parent, &Children), With<Online>>,
    inventories: Query<Entity, With<Inventory>>,
    tiles: Query<&Children, With<Tile>>,
    items: Query<(Entity, &Item), With<CanTake>>,
) {
    for command in commands.iter() {
        if let Command::Take((target, all)) = &command.command {
            let Some((client, tile, children)) = players.iter_mut().find(|(c, _, _)| c.id == command.from) else {
                debug!("Could not find authenticated client: {:?}", command.from);

                continue;
            };

            let Ok(siblings) = tiles.get(tile.get()) else {
                debug!("Could not get tile: {:?}", tile.get());

                continue;
            };

            let Some(inventory) = children.iter().find_map(|child| inventories.get(*child).ok()) else {
                debug!("Could not get inventory for client: {:?}", client);

                continue;
            };

            let mut items_found = siblings
                .iter()
                .filter_map(|sibling| items.get(*sibling).ok())
                .filter(|(_, item)| {
                    item.name.to_lowercase() == *target
                        || item.short_name.to_lowercase() == *target
                        || item.tags.contains(target)
                })
                .collect::<Vec<(Entity, &Item)>>();

            if !*all {
                items_found.truncate(1);
            }

            items_found.iter().for_each(|(entity, _)| {
                bevy.entity(*entity).set_parent(inventory);
            });

            let item_names = item_name_list(
                &items_found
                    .iter()
                    .map(|(_, item)| item.name.clone())
                    .collect::<Vec<String>>(),
            );

            if item_names.is_empty() {
                outbox.send_text(client.id, format!("You don't see a {target} here."));
            } else {
                outbox.send_text(client.id, format!("You take {item_names}."));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test::{
        app_builder::AppBuilder,
        item_builder::ItemBuilder,
        player_builder::PlayerBuilder,
        tile_builder::{TileBuilder, ZoneBuilder},
        utils::{get_message_content, send_message},
    };

    use super::*;

    #[test]
    fn by_name() {
        let mut app = AppBuilder::new().build();
        app.add_system(take);

        let zone = ZoneBuilder::new().build(&mut app);
        let tile = TileBuilder::new().build(&mut app, zone);

        ItemBuilder::new()
            .name("stick")
            .can_take()
            .tile(tile)
            .build(&mut app);

        ItemBuilder::new()
            .name("rock")
            .can_take()
            .tile(tile)
            .build(&mut app);

        let (_, client_id, inventory) = PlayerBuilder::new()
            .tile(tile)
            .has_inventory()
            .build(&mut app);

        send_message(&mut app, client_id, "take stick");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert_eq!(content, format!("You take a stick."));

        assert_eq!(
            app.world.get::<Children>(inventory.unwrap()).unwrap().len(),
            1
        );

        assert_eq!(app.world.get::<Children>(tile).unwrap().len(), 2);
    }

    #[test]
    fn by_tag() {
        let mut app = AppBuilder::new().build();
        app.add_system(take);

        let zone = ZoneBuilder::new().build(&mut app);
        let tile = TileBuilder::new().build(&mut app, zone);

        ItemBuilder::new()
            .name("stick")
            .tags(vec!["weapon"])
            .can_take()
            .tile(tile)
            .build(&mut app);

        let (_, client_id, inventory) = PlayerBuilder::new()
            .tile(tile)
            .has_inventory()
            .build(&mut app);

        send_message(&mut app, client_id, "take weapon");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert_eq!(content, format!("You take a stick."));

        assert_eq!(
            app.world.get::<Children>(inventory.unwrap()).unwrap().len(),
            1
        );

        assert_eq!(app.world.get::<Children>(tile).unwrap().len(), 1);
    }

    #[test]
    fn all() {
        let mut app = AppBuilder::new().build();
        app.add_system(take);

        let zone = ZoneBuilder::new().build(&mut app);
        let tile = TileBuilder::new().build(&mut app, zone);

        ItemBuilder::new()
            .name("stick")
            .can_take()
            .tile(tile)
            .build(&mut app);

        ItemBuilder::new()
            .name("stick")
            .can_take()
            .tile(tile)
            .build(&mut app);

        ItemBuilder::new()
            .name("rock")
            .can_take()
            .tile(tile)
            .build(&mut app);

        let (_, client_id, inventory) = PlayerBuilder::new()
            .tile(tile)
            .has_inventory()
            .build(&mut app);

        send_message(&mut app, client_id, "take all stick");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert_eq!(content, format!("You take 2 sticks."));

        assert_eq!(
            app.world.get::<Children>(inventory.unwrap()).unwrap().len(),
            2
        );

        assert_eq!(app.world.get::<Children>(tile).unwrap().len(), 2);
    }

    #[test]
    fn not_found() {
        let mut app = AppBuilder::new().build();
        app.add_system(take);

        let zone = ZoneBuilder::new().build(&mut app);
        let tile = TileBuilder::new().build(&mut app, zone);

        let (_, client_id, inventory) = PlayerBuilder::new()
            .tile(tile)
            .has_inventory()
            .build(&mut app);

        send_message(&mut app, client_id, "take sword");
        app.update();

        let content = get_message_content(&mut app, client_id);

        assert_eq!(content, format!("You don't see a sword here."));

        assert!(app.world.get::<Children>(inventory.unwrap()).is_none());
    }
}
