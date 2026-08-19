//! User model.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_with::{TryFromInto, serde_as};

use utoipa::{
    PartialSchema, ToSchema,
    openapi::{ObjectBuilder, RefOr, Type, schema::Schema},
};

use bytemuck::cast;

/// A single user.
#[serde_as]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
pub struct User {
    /// The short ID of the user.
    pub id: String,
    /// The display name of the user.
    pub display_name: String,
    /// The ID of the associated Discord user, if it exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_user_id: Option<i64>,
    /// The user's flags.
    #[serde_as(as = "TryFromInto<i32>")]
    pub flags: UserFlags,
}

bitflags::bitflags! {
    /// User flags.
    #[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
    pub struct UserFlags: u32 {
        /// The user is an updater.
        const MAINTAINER = 0x01;
    }
}

impl PartialSchema for UserFlags {
    fn schema() -> RefOr<Schema> {
        const DESCRIPTION: &str = r#"
User flags:
* MAINTAINER = 0x01"#;
        Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .description(Some(DESCRIPTION))
                .build(),
        )
        .into()
    }
}

impl ToSchema for UserFlags {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("UserFlags")
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((UserFlags::name().into(), UserFlags::schema()));
    }
}

impl From<i32> for UserFlags {
    fn from(value: i32) -> Self {
        let value: u32 = cast(value);
        UserFlags::from_bits_truncate(value)
    }
}

impl From<UserFlags> for i32 {
    fn from(value: UserFlags) -> Self {
        cast(value.bits())
    }
}
