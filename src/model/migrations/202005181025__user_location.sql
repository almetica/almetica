CREATE TABLE "user_location"
(
    "user_id" INT NOT NULL UNIQUE REFERENCES "user" ON DELETE CASCADE,
    "zone"    INT NOT NULL,
    "location_x" FLOAT NOT NULL,
    "location_y" FLOAT NOT NULL,
    "location_z" FLOAT NOT NULL,
    "rotation" SMALLINT NOT NULL
);
