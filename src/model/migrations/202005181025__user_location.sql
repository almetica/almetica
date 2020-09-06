CREATE TABLE "user_location"
(
    "user_id" INT NOT NULL UNIQUE REFERENCES "user" ON DELETE CASCADE,
    "zone_id"    INT NOT NULL,
    "location_x" REAL NOT NULL,
    "location_y" REAL NOT NULL,
    "location_z" REAL NOT NULL,
    "rotation_x" REAL NOT NULL,
    "rotation_y" REAL NOT NULL,
    "rotation_z" REAL NOT NULL
);
