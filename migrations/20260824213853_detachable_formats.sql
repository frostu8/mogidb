PRAGMA legacy_alter_table = ON;

-- RECREATE THIS BULLSHIT BECAUSE SQLITE SUCKS!!!!!
CREATE TABLE event_format_new (
    id INTEGER PRIMARY KEY,
    room_id INTEGER REFERENCES room(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    team_mode INTEGER NOT NULL DEFAULT 0,
    inserted_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

INSERT INTO event_format_new (id, room_id, name, team_mode, inserted_at, updated_at)
SELECT id, room_id, name, team_mode, inserted_at, updated_at
FROM event_format;

DROP TABLE event_format;

ALTER TABLE event_format_new RENAME TO event_format;

CREATE UNIQUE INDEX event_format_room_name
    ON event_format (room_id, name)
    WHERE room_id IS NOT NULL;
