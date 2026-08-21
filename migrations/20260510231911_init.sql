-- Each Discord guild has a global config.
CREATE TABLE guild (
    id INTEGER PRIMARY KEY,
    discord_guild_id BIGINT NOT NULL UNIQUE,
    -- These are the default settings for rooms in this guild.
    settings TEXT NOT NULL,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- Each guild has many channels called "rooms"
CREATE TABLE room (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    discord_channel_id BIGINT NOT NULL UNIQUE,
    -- The name of the channel.
    name VARCHAR(255) NOT NULL,
    -- If events can be played in this room.
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    -- Settings overrides
    overrides TEXT NOT NULL,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- Each guild also has many servers
CREATE TABLE server (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    -- The address of the server
    remote VARCHAR(255) NOT NULL,
    -- The label
    label VARCHAR(255) NOT NULL,
    -- A user-defined note
    note VARCHAR(255),
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    CONSTRAINT unique_label UNIQUE (guild_id, label),
    CONSTRAINT unique_remote UNIQUE (guild_id, remote)
);

-- Each room has >0 formats.
CREATE TABLE event_format (
    id INTEGER PRIMARY KEY,
    -- The room this format is a part of.
    room_id INTEGER NOT NULL REFERENCES room(id) ON DELETE CASCADE,
    -- The name of the format.
    name VARCHAR(255) NOT NULL,
    -- Team balancing mode of the format
    -- 0 - FFA
    -- 1 - 2 Teams
    -- 2 - 4 Teams
    team_mode INTEGER NOT NULL DEFAULT 0,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    UNIQUE (room_id, name)
);

-- Each format "event_format" has >0 servers.
CREATE TABLE format_server (
    id INTEGER PRIMARY KEY,
    event_format_id INTEGER NOT NULL REFERENCES event_format(id) ON DELETE CASCADE,
    server_id INTEGER NOT NULL REFERENCES server(id) ON DELETE CASCADE,

    UNIQUE (event_format_id, server_id)
);
