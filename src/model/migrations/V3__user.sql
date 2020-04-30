CREATE TYPE gender AS ENUM ('male', 'female');
CREATE TYPE race AS ENUM ('human', 'castanic', 'aman', 'high elf', 'elin popori', 'baraka');
CREATE TYPE user_class AS ENUM ('warrior', 'lancer', 'slayer', 'berserker', 'sorcerer', 'archer', 'priest', 'elementalist', 'soulless', 'engineer', 'fighter', 'ninja', 'valkyrie');

CREATE TABLE account_user
(
    id          SERIAL     PRIMARY KEY,
    account_id  BIGINT     NOT NULL REFERENCES account ON DELETE CASCADE,
    name        TEXT       NOT NULL UNIQUE,
    gender      gender     NOT NULL,
    race        race       NOT NULL,
    user_class  user_class NOT NULL,
    shape       BYTEA      NOT NULL,
    details     BYTEA      NOT NULL,
    appearance  BYTEA      NOT NULL,
    appearance2 BYTEA      NOT NULL,
    playtime    BIGINT     NOT NULL      DEFAULT 0,
    created_at  TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);
