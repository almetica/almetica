CREATE TYPE "gender" AS ENUM ('male', 'female');
CREATE TYPE "race" AS ENUM ('human', 'castanic', 'aman', 'high elf', 'elin popori', 'baraka');
CREATE TYPE "user_class" AS ENUM ('warrior', 'lancer', 'slayer', 'berserker', 'sorcerer', 'archer', 'priest', 'elementalist', 'soulless', 'engineer', 'fighter', 'ninja', 'valkyrie');

CREATE TABLE "user"
(
    "id"                 SERIAL PRIMARY KEY,
    "account_id"         BIGINT     NOT NULL REFERENCES "account" ON DELETE CASCADE,
    "name"               TEXT       NOT NULL UNIQUE,

    "gender"             gender     NOT NULL,
    "race"               race       NOT NULL,
    "class"              user_class NOT NULL,
    "shape"              BYTEA      NOT NULL,
    "details"            BYTEA      NOT NULL,
    "appearance"         BYTEA      NOT NULL,
    "appearance2"        INT        NOT NULL      DEFAULT 0,

    "level"              INT        NOT NULL      DEFAULT 1,
    "awakening_level"    INT        NOT NULL      DEFAULT 0,
    "laurel"             INT        NOT NULL      DEFAULT 0,
    "achievement_points" INT        NOT NULL      DEFAULT 0,
    "playtime"           BIGINT     NOT NULL      DEFAULT 0,
    "rest_bonus_xp"      BIGINT     NOT NULL      DEFAULT 0,
    "show_face"          BOOLEAN    NOT NULL      DEFAULT FALSE,
    "show_style"         BOOLEAN    NOT NULL      DEFAULT FALSE,

    "lobby_slot"         INT        NOT NULL      DEFAULT 0,
    "is_new_character"   BOOLEAN    NOT NULL      DEFAULT FALSE,
    "tutorial_state"     INT        NOT NULL      DEFAULT 0,

    "is_deleting"        BOOLEAN    NOT NULL      DEFAULT FALSE,
    "delete_at"          TIMESTAMP WITH TIME ZONE,

    "last_logout_at"     TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    "created_at"         TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);
