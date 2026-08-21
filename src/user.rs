//! User operations.

use chrono::{DateTime, Utc};
use mogidb_model::user::{User, UserFlags};

use rand::{Rng, SeedableRng as _};

use sqlx::{FromRow, SqliteConnection};

use crate::{
    error::{Error, NotFound},
    short_id,
};

#[derive(Clone, Debug, FromRow)]
pub struct UserEntity {
    pub id: i32,
    pub short_id: String,
    pub display_name: String,
    #[sqlx(try_from = "i32")]
    pub flags: UserFlags,
    pub discord_user_id: Option<i64>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<UserEntity> for User {
    fn from(value: UserEntity) -> Self {
        User {
            id: value.short_id,
            display_name: value.display_name,
            discord_user_id: value.discord_user_id,
            flags: value.flags,
        }
    }
}

/// A builder for a user.
#[derive(Debug)]
pub struct UserBuilder {
    display_name: String,
    flags: UserFlags,
    discord_user_id: Option<i64>,
}

impl UserBuilder {
    /// Creates a new `UserBuilder`.
    pub fn new(display_name: impl Into<String>) -> UserBuilder {
        UserBuilder {
            display_name: display_name.into(),
            flags: UserFlags::empty(),
            discord_user_id: None,
        }
    }

    /// Sets the new user's flags.
    pub fn flags(self, flags: UserFlags) -> UserBuilder {
        UserBuilder { flags, ..self }
    }

    /// Sets the discord ID of the user.
    pub fn discord_user_id(self, id: i64) -> UserBuilder {
        UserBuilder {
            discord_user_id: Some(id),
            ..self
        }
    }

    /// Creates the user.
    pub async fn create(self, conn: &mut SqliteConnection) -> Result<UserEntity, Error> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(rand::random());
        self.create_with(conn, &mut rng).await
    }

    /// Creates the user with a given PRNG.
    pub async fn create_with<R>(
        self,
        conn: &mut SqliteConnection,
        rng: &mut R,
    ) -> Result<UserEntity, Error>
    where
        R: Rng,
    {
        let Self {
            display_name,
            flags,
            discord_user_id,
            ..
        } = self;
        let now = Utc::now();

        short_id::allocate_with(rng)
            .length(6)
            // try to insert with short_id
            .insert(conn, async move |short_id, conn| {
                sqlx::query_as::<_, UserEntity>(
                    r#"
                    INSERT INTO user
                        (
                            inserted_at,
                            updated_at,
                            short_id,
                            display_name,
                            flags,
                            discord_user_id
                        )
                    VALUES ($1, $1, $2, $3, $4, $5)
                    RETURNING
                        id,
                        short_id,
                        display_name,
                        flags,
                        discord_user_id,
                        inserted_at,
                        updated_at
                    "#,
                )
                .bind(now)
                .bind(short_id)
                .bind(&display_name)
                .bind(i32::from(flags))
                .bind(discord_user_id)
                .fetch_one(conn)
                .await
            })
            .await
    }
}

/// Gets a user by their discord ID.
pub async fn get_user_by_discord_id(
    discord_user_id: i64,
    conn: &mut SqliteConnection,
) -> Result<Option<UserEntity>, Error> {
    sqlx::query_as::<_, UserEntity>(
        r#"
        SELECT *
        FROM user
        WHERE discord_user_id = $1
        "#,
    )
    .bind(discord_user_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
}

/// Gets a user by their short ID.
pub async fn get_user(short_id: &str, conn: &mut SqliteConnection) -> Result<UserEntity, Error> {
    sqlx::query_as::<_, UserEntity>(
        r#"
        SELECT *
        FROM user
        WHERE short_id = $1
        "#,
    )
    .bind(short_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Error::new)
    .and_then(|user| user.ok_or_else(|| NotFound::User(short_id.to_owned()).into()))
}
