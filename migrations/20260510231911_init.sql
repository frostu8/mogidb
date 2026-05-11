-- Each Discord guild has a global config.
CREATE TABLE guild (
    id INTEGER PRIMARY KEY,
    discord_guild_id BIGINT NOT NULL UNIQUE,
    -- These are the default settings for rooms in this guild.
    players_required INTEGER NOT NULL DEFAULT 8,
    format_selection_mode INTEGER NOT NULL DEFAULT 0,
    votes_required INTEGER NOT NULL DEFAULT 4,
    decay_after INTEGER NOT NULL DEFAULT 3000,
    inactivity_warning_after INTEGER NOT NULL DEFAULT 1500,
    inactivity_drop_after INTEGER NOT NULL DEFAULT 2100,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    CONSTRAINT chk_guild_players_required CHECK (players_required > 0),
    CONSTRAINT chk_guild_votes_required CHECK (players_required > 0),
    CONSTRAINT chk_guild_decay_after CHECK (decay_after >= 0),
    CONSTRAINT chk_guild_inactivity_warning_after CHECK (inactivity_warning_after >= 0),
    CONSTRAINT chk_guild_inactivity_drop_after CHECK (inactivity_drop_after >= 0)
);

-- Each guild has many channels called "rooms"
CREATE TABLE room (
    id INTEGER PRIMARY KEY,
    parent_id INTEGER NOT NULL REFERENCES guild(id),
    discord_channel_id BIGINT NOT NULL UNIQUE,
    -- If events can be played in this room.
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    -- How many players are required to start a mogi.
    players_required INTEGER NOT NULL DEFAULT 8,
    -- Whether formats should be selected randomly or voted.
    -- 0 - VOTE
    -- 1 - RANDOM
    format_selection_mode INTEGER NOT NULL DEFAULT 0,
    -- How many votes a format needs to be selected.
    votes_required INTEGER NOT NULL DEFAULT 4,
    decay_after INTEGER NOT NULL DEFAULT 3000,
    inactivity_warning_after INTEGER NOT NULL DEFAULT 1500,
    inactivity_drop_after INTEGER NOT NULL DEFAULT 2100,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    CONSTRAINT chk_guild_players_required CHECK (players_required > 0),
    CONSTRAINT chk_guild_votes_required CHECK (players_required > 0),
    CONSTRAINT chk_room_decay_after CHECK (decay_after >= 0),
    CONSTRAINT chk_room_inactivity_warning_after CHECK (inactivity_warning_after >= 0),
    CONSTRAINT chk_room_inactivity_drop_after CHECK (inactivity_drop_after >= 0)
);

-- Each guild also has many servers
CREATE TABLE server (
    id INTEGER PRIMARY KEY,
    guild_id INTEGER NOT NULL REFERENCES guild(id),
    -- The address of the server
    remote VARCHAR(255) NOT NULL,
    -- The label
    label VARCHAR(255),
    -- A user-defined note
    note VARCHAR(255),
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,

    UNIQUE (guild_id, label)
);

-- Each room has >0 formats.
CREATE TABLE event_format (
    id INTEGER PRIMARY KEY,
    -- The room this format is a part of.
    room_id INTEGER NOT NULL REFERENCES room(id),
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
    event_format_id INTEGER NOT NULL REFERENCES event_format(id),
    server_id INTEGER NOT NULL REFERENCES server(id),

    UNIQUE (event_format_id, server_id)
);
