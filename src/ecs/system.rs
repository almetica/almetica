pub mod event_handler;
//pub mod event_dispatcher;

// TODO The connection system needs to take the connection registration and create
// a ID for the connection. This ID needs to be send back to the connection and needs
// to be used after this point for every event. The system saves the id -> channel
// mapping into a HashMap resource. This connection ID could later serves as the
// user ID for all intense and purposes.
