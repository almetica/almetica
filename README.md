# Almetica

[![Gitter](https://badges.gitter.im/almetica-server/community.svg)](https://gitter.im/almetica-server/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)

This is a server for the MMORPG TERA written in rust. Currently targeting TERA EU 93.02.

## Requirements

A stable rust installation (version 1.42+).

## Building

Developer build:

```bash
cargo build
```

For hardware accelerated AES you will need to use a compatible CPU and this build command:

```bash
RUSTFLAGS="-C target-feature=+aes,+ssse3" cargo build
```

For the best performance (including AES speed improvements) compile the server with the full
native instruction set of your CPU:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build
```

Remember to use the ```--release``` flag if you want to activate all compiler optimizations.

## Running

Configure the server with the help of the provided configuration template
(config.yaml.tmpl). In your data folder you need currently following files:
 * messages.yaml 
   (A YAML list with all system messages in the same order as the client.)
 * opcocode.yaml
   (A YAML hashmap with the packet name as key and the opcode value as the value
   as defined in the client.)
 * integrity.yaml
   (A YAML list with all packet names that need the integrity check (>= version 93))

The configuration file also need the key and IV of the TERA datacenter file you
are using. You need to extract the information out of the TERA client file you
are targeting.

We will provide tools / instructions how to do so in the future.

You can run the server with the following commands:

```bash
RUST_LOG=info cargo run --bin almetica
```

## Testing

Since some tests are integration tests that need a postgres database, you need to
configure a database connection which will be used for the testing. You need a 
database user that is allowed to create and delete databases and I recommend just
to spin up a docker container for the testing. Don't run the tests against your
production database.

The tests will create a randomly named test database so that they can run in 
parallel.

To configure the database access, please create a .env file in the project root
and add a TEST_DATABASE_CONNECTION variable.

Use the format that is documented here:

https://docs.rs/postgres/0.17.2/postgres/config/struct.Config.html
 
## Contributing

Please contacts us in advance if you want to help with the server development so
that we don't work on the same stuff at the same time.

Always write tests for the stuff you program. Code without tests will not be
included.

## FAQ

### What are your goals?

Provide a server platform for TERA that gives better performance then the original
server while providing the same feature set.

I don't strive to emulate the original TERA server 100% the same. We will
optimize / improve functionality where it seems logical (for example stricter
validation of client commands).

This server should also act as a way to preserve TERA for the future.

### Why didn't you extend already existing server projects:

I had four requirements for the server projects for me to consider continue
developing them:

 * Open Source License
 * Some kind of tests (unit / integration test etc.)
 * Written in a compiled and typed language
 * Written with a clear design goal

None of he evaluated existing server projects did fulfilled these requirements.

## License

Licensed under AGPL version 3.

The GNU Affero General Public License is based on the GNU GPL, but has an
additional term to allow users who interact with the licensed software over a
network to receive the source for that program. We recommend that people
consider using the GNU AGPL for any software which will commonly be run over a
network.

## Credits

It's pretty hard to trace the origin of some of the achievements. So I will just
credit people without their specific contributions. Most of them did discover
specific issues while reverse engineering the TERA network protocol or did some
other kind of groundwork that this sever is based on (ordered alphabetically):

alexrp, caali-hackerman, mirrawrs, meishu, P5yl0, pinkiepie
