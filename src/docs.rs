//! OpenAPI documentation.

use mogidb_model::{
    error::ApiError,
    event::{Event, EventFormat, EventParticipant, FormatSelectionMode},
    guild::Guild,
    request::TeamBalanceMode,
    response::JoinEventResponse,
    room::{Room, RoomOptions, RoomOptionsOverrides},
    server::{GameServer, GameSpeed, PlayerInfo, ServerInfo},
    user::{User, UserFlags},
};
use utoipa::OpenApi;
use utoipa::{Modify, openapi::security::{ApiKey, ApiKeyValue, SecurityScheme}};

use crate::routes::{
    guild::{
        self, CreateGuildRequest, UpdateGuildRequest, UpdateRoomSettings,
        room::{
            CreateRoomRequest, UpdateRoomRequest,
            event::{CreateEventRequest, participants::{AssignTeamsRequest, JoinEventRequest}},
            format::CreateEventFormatRequest,
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
    modifiers(&SecurityAddon),
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
        guild::room::event::participants::list,
        guild::room::event::participants::join,
        guild::room::event::participants::leave,
        guild::room::event::participants::assign_teams,
        guild::server::create,
        guild::server::list,
        guild::server::show,
        guild::server::update,
        guild::server::delete,
        guild::room::format::create,
        guild::room::format::list,
        guild::room::format::show,
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
        EventParticipant,
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
        JoinEventRequest,
        JoinEventResponse,
        AssignTeamsRequest,
        TeamBalanceMode,
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

/// Adds the API key security scheme to the documentation.
///
/// All routes are guarded by the auth middleware (`X-API-KEY` header), so the
/// requirement is applied globally here rather than per-operation.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-KEY"))),
        );
        openapi.security = Some(vec![utoipa::openapi::SecurityRequirement::new(
            "api_key",
            Vec::<String>::new(),
        )]);
    }
}

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
