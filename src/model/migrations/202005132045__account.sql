CREATE TYPE "password_hash_algorithm" AS ENUM ('argon2');

CREATE TABLE "account"
(
    "id"         BIGSERIAL PRIMARY KEY,
    "name"       TEXT                    NOT NULL UNIQUE,
    "password"   TEXT                    NOT NULL,
    "algorithm"  password_hash_algorithm NOT NULL,
    "created_at" TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE OR REPLACE FUNCTION account_update_updated_at()
    RETURNS TRIGGER AS
$$
BEGIN
    NEW."updated_at" = now();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER account_update_timestamp
    BEFORE UPDATE
    ON "account"
    FOR EACH ROW
EXECUTE PROCEDURE account_update_updated_at();
