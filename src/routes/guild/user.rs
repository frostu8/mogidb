//! User routes and access

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use garde::Validate;

use mogidb_model::{error::ApiError, user::User};

use serde::Deserialize;

use utoipa::ToSchema;

use crate::{
    AppState,
    error::Error,
    json::Json,
    user::{UserBuilder, get_user, get_user_by_discord_id},
    validate::Valid,
};

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[garde(context(AppState as state))]
pub struct UpsertUserRequest {
    #[garde(length(min = 1, max = 255))]
    pub display_name: String,
}

/// A route used to upsert users into the MogiDB.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/users/{discord_user_id}",
    tag = "user",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("discord_user_id" = i64, Path, description = "Discord user id"),
    ),
    request_body = UpsertUserRequest,
    responses(
        (status = OK, description = "The existing user", body = User),
        (status = NO_CONTENT, description = "The newly created user", body = User),
        (status = BAD_REQUEST, description = "Bad request", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn upsert(
    Path((guild_id, discord_user_id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Valid(Json(request)): Valid<Json<UpsertUserRequest>>,
) -> Result<(StatusCode, Json<User>), Error> {
    let mut tx = state.db.begin().await.map_err(Error::new)?;
    let now = Utc::now();

    let res = sqlx::query_as::<_, (i32,)>("SELECT id FROM guild WHERE discord_guild_id = $1")
        .bind(guild_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::new)?;
    let Some((guild_id,)) = res else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id,
        )));
    };

    // Try to find user first
    let user = get_user_by_discord_id(guild_id, discord_user_id, &mut *tx).await?;
    if let Some(mut user) = user {
        let mut should_update = false;
        // Check if fields need updating
        if user.display_name != request.display_name {
            should_update = true;
            user.display_name = request.display_name.clone();
        }

        // Commit
        if should_update {
            sqlx::query(
                r#"
                UPDATE user
                SET display_name = $3, updated_at = $2
                WHERE id = $1
                "#,
            )
            .bind(user.id)
            .bind(now)
            .bind(&user.display_name)
            .execute(&mut *tx)
            .await
            .map_err(Error::new)?;
        }

        tx.commit().await.map_err(Error::new)?;

        Ok((StatusCode::OK, Json(User::from(user))))
    } else {
        // Try to create user
        let user = UserBuilder::new(guild_id, &request.display_name)
            .discord_user_id(discord_user_id)
            .create(&mut *tx)
            .await?;

        tx.commit().await.map_err(Error::new)?;

        Ok((StatusCode::CREATED, Json(User::from(user))))
    }
}

/// Fetches the user with the given short ID.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/users/{user_id}",
    tag = "user",
    params(
        ("guild_id" = i64, Path, description = "Discord guild id"),
        ("user_id" = String, Path, description = "The id of the user"),
    ),
    responses(
        (status = OK, description = "The user", body = User),
        (status = BAD_REQUEST, description = "Bad request", body = ApiError),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error", body = ApiError),
    )
)]
pub async fn show(
    Path((guild_id, user_id)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> Result<Json<User>, Error> {
    let mut conn = state.db.acquire().await.map_err(Error::new)?;

    let res = sqlx::query_as::<_, (i32,)>("SELECT id FROM guild WHERE discord_guild_id = $1")
        .bind(guild_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::new)?;
    let Some((guild_id,)) = res else {
        return Err(Error::not_found(format_args!(
            "guild {} not found",
            guild_id,
        )));
    };

    let user = get_user(&user_id, &mut *conn).await?;
    let Some(user) = user else {
        return Err(Error::not_found(format_args!("user {} not found", user_id)));
    };

    // Check for mismatched guild IDs
    if guild_id != user.guild_id {
        return Err(Error::not_found(format_args!("user {} not found", user_id)));
    }

    Ok(Json(User::from(user)))
}
