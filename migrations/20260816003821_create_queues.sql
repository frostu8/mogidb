-- There are many users.
CREATE TABLE user (
    id INTEGER PRIMARY KEY,
    -- A short ID used to uniquely identify a user.
    short_id CHAR(6) UNIQUE NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    -- A set of flags for the user.
    flags INTEGER NOT NULL DEFAULT 0,
    -- The user's discord snowflake.
    discord_user_id BIGINT,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE profile (
    id INTEGER PRIMARY KEY,
    -- The parent user ID of the profile.
    parent_id INTEGER REFERENCES user(id),
    -- The public key of their profile.
    public_key BINARY(32) NOT NULL UNIQUE,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- A room can have many events.
CREATE TABLE event (
    id INTEGER PRIMARY KEY,
    -- A short ID used to uniquely identify the event..
    short_id CHAR(9) UNIQUE NOT NULL,
    room_id INTEGER NOT NULL REFERENCES room(id),
    -- A user-defined title for the event.
    title VARCHAR(255),
    -- The stage of the event.
    status INTEGER NOT NULL DEFAULT 0,
    -- Sorry, your verse has been rejected, because it was an offkey and
    -- offbeat mess like I expected.
    -- Prevents scoring if an event is rejected.
    rejected BOOLEAN NOT NULL DEFAULT FALSE,
    format_id INTEGER REFERENCES event_format(id),
    server_id INTEGER REFERENCES server(id),
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- A event can have many participants.
CREATE TABLE event_participant (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES user(id),
    event_id INTEGER NOT NULL REFERENCES event(id),
    team_number INTEGER,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    UNIQUE (user_id, event_id)
);

-- An event can have many rounds.
CREATE TABLE round (
    id INTEGER PRIMARY KEY,
    event_id INTEGER NOT NULL REFERENCES event(id),
    -- The sequence number of the round. Used to force a certain sequence.
    sequence INTEGER NOT NULL DEFAULT 0,
    -- The level id played on the round
    level_id VARCHAR(255) NOT NULL,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- A round can have many participants
CREATE TABLE round_participant (
    id INTEGER PRIMARY KEY,
    round_id INTEGER NOT NULL REFERENCES round(id),
    -- The user this record is for.
    -- May be null if the user hasn't been linked yet.
    user_id INTEGER REFERENCES user(id),
    -- The profile for this user.
    profile_id INTEGER NOT NULL REFERENCES profile(id),
    -- Actual round data.
    -- For battle, this is the score of the player. For race, this is the
    -- player's EXP.
    score INTEGER NOT NULL DEFAULT 0,

    UNIQUE (round_id, user_id),
    UNIQUE (round_id, profile_id)
);
