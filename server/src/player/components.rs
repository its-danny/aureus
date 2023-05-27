use bevy::prelude::*;
use bevy_nest::server::ClientId;

use super::config::CharacterConfig;

#[derive(Component)]
pub struct Client {
    pub id: ClientId,
    pub width: u16,
}

#[derive(Component)]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub role: i16,
    pub config: CharacterConfig,
}

impl Character {
    pub fn can(&self, permission: i16) -> bool {
        self.role & permission == permission
    }
}

#[cfg(test)]
mod tests {
    use crate::player::permissions::*;

    use super::*;

    #[test]
    fn test_permissions() {
        let admin = Character {
            id: 0,
            name: "admin".to_string(),
            role: TELEPORT,
            config: CharacterConfig { brief: false },
        };

        assert!(admin.can(TELEPORT));

        let player = Character {
            id: 0,
            name: "player".to_string(),
            role: 0,
            config: CharacterConfig { brief: false },
        };

        assert!(!player.can(TELEPORT));
    }
}
