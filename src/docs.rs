//! OpenAPI documentation.

use mogidb_model::{
    error::ApiError,
    event::FormatSelectionMode,
    guild::Guild,
    room::{Room, RoomOptions, RoomOptionsOverrides},
    server::{GameServer, GameSpeed, PlayerInfo, ServerInfo},
};
use utoipa::OpenApi;

use crate::routes::guild::{
    self, CreateGuildRequest, UpdateGuildRequest, UpdateRoomSettings,
    room::{CreateRoomRequest, UpdateRoomRequest},
    server::CreateServerRequest,
};

/// OpenAPI documentation for `mogidb`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "MogiDB",
        description = "Gutbuster backend database and API."
    ),
    paths(
        guild::create,
        guild::show,
        guild::update,
        guild::room::create,
        guild::room::show,
        guild::room::update,
        guild::room::delete,
        guild::server::create,
        guild::server::show,
        guild::server::delete,
    ),
    components(schemas(
        ApiError,
        FormatSelectionMode,
        Guild,
        Room,
        RoomOptions,
        RoomOptionsOverrides,
        GameServer,
        GameSpeed,
        PlayerInfo,
        ServerInfo,
        CreateGuildRequest,
        UpdateGuildRequest,
        UpdateRoomSettings,
        CreateRoomRequest,
        UpdateRoomRequest,
        CreateServerRequest,
    )),
    tags(
        (name = "guild", description = "Guild management"),
        (name = "room", description = "Room management"),
        (name = "server", description = "Game server management"),
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use std::fs;

    use utoipa::OpenApi as _;

    #[test]
    fn openapi_generates() {
        let doc = crate::docs::ApiDoc::openapi();
        let yaml = serde_norway::to_string(&doc).unwrap();
        fs::write("/tmp/openapi.yaml", &yaml).unwrap();
        // sanity checks
        assert!(yaml.contains("/guilds"));
        assert!(yaml.contains("/guilds/{guild_id}/rooms/{room_id}"));
        assert!(yaml.contains("/guilds/{guild_id}/servers/{server_id}"));
        assert!(yaml.contains("Guild"));
        assert!(yaml.contains("Room"));
        assert!(yaml.contains("GameServer"));
    }
}
