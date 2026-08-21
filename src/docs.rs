//! OpenAPI documentation.

use mogidb_model::{
    error::ApiError,
    event::{Event, EventFormat, FormatSelectionMode},
    guild::Guild,
    room::{Room, RoomOptions, RoomOptionsOverrides},
    server::{GameServer, GameSpeed, PlayerInfo, ServerInfo},
    user::{User, UserFlags},
};
use utoipa::OpenApi;

use crate::routes::{
    guild::{
        self, CreateGuildRequest, UpdateGuildRequest, UpdateRoomSettings,
        room::{
            CreateRoomRequest, UpdateRoomRequest,
            event::CreateEventRequest,
            format::{CreateEventFormatRequest, UpdateEventFormatRequest},
        },
        server::{CreateServerRequest, UpdateServerRequest},
    },
    user::{self, UpsertUserRequest},
};

/// OpenAPI documentation for `mogidb`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "MogiDB",
        description = "Gutbuster backend database and API."
    ),
    paths(
        user::upsert,
        user::show,
        guild::create,
        guild::show,
        guild::update,
        guild::event::list,
        guild::room::create,
        guild::room::show,
        guild::room::update,
        guild::room::delete,
        guild::room::event::create,
        guild::room::event::show,
        guild::room::event::show_current,
        guild::room::event::update,
        guild::room::event::delete,
        guild::server::create,
        guild::server::show,
        guild::server::update,
        guild::server::delete,
        guild::room::format::create,
        guild::room::format::list,
        guild::room::format::show,
        guild::room::format::update,
        guild::room::format::delete,
    ),
    components(schemas(
        ApiError,
        FormatSelectionMode,
        Guild,
        Room,
        RoomOptions,
        RoomOptionsOverrides,
        Event,
        EventFormat,
        GameServer,
        GameSpeed,
        PlayerInfo,
        ServerInfo,
        User,
        UserFlags,
        CreateGuildRequest,
        UpdateGuildRequest,
        UpdateRoomSettings,
        CreateRoomRequest,
        UpdateRoomRequest,
        CreateServerRequest,
        UpdateServerRequest,
        CreateEventRequest,
        CreateEventFormatRequest,
        UpdateEventFormatRequest,
        UpsertUserRequest,
    )),
    tags(
        (name = "guild", description = "Guild management"),
        (name = "room", description = "Room management"),
        (name = "server", description = "Game server management"),
        (name = "user", description = "User management"),
        (name = "event", description = "Events creation and management"),
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
