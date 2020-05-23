![Almetica Logo](assets/logo_wide.svg)

[![Gitter](https://badges.gitter.im/almetica-server/community.svg)](https://gitter.im/almetica-server/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![LOC](https://tokei.rs/b1/github/almetica/almetica?category=lines)](https://github.com/almetica/almetica)

This is a server for the MMORPG TERA written in rust. Currently targeting TERA EU 93.04.

To connect to the server you need to use the [custom client launcher](https://github.com/almetica/almetica-launcher). It's a direct replacement for the publisher launcher and Tl.exe.

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

## Configuration

Configure the server with the help of the provided configuration template
(config.yaml.tmpl). 

You also need some additional files that you need to extract yourself from the
TERA client. We will provide tools / instructions how to do so in the future.

You can find these tools yourself though on Github.

### integrity.yaml

A YAML file with a list of all packet names that need the integrity check (>= version 93).

Format:
```yaml
- C_CAST_FISHING_ROD
- C_DIALOG
...
```

### key.yaml
A YAML file with two keys: "key" and "iv". These are the parts of the AES256 key
which is used to decrypt the datacenter file. Extracted from the memory while
the TERA client is runnig.

Format:
```yaml
key: E1B1C4666F64681889BC8A5594387E2D
iv: 1F494C6BB424C916CA44BB1C64CEAA41
...
```

### messages.yaml 
A YAML file with a list of all system messages in the same order as the client.

Format:
```yaml
- SMT_UNDEFINED
- SMT_LOBBY_CANNOT_CONNECT
...
```

### opcocode.yaml
A YAML file with a hashmap of the packet name as key and the opcode value as the
value as defined in the client.

Format:
```yaml
C_ACCEPT_CONTRACT: 12345
C_ACCEPT_FRIEND: 67890
...
```

## Running

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
and add a TEST_DATABASE_CONNECTION variable:

```bash
TEST_DATABASE_CONNECTION="postgres://username:password@192.168.1.1:5432"
```

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

## Legal

This software contains no code from the original game. We are developing an
alternative server implementation like the open source server implementation
for Lineage 2: L2J, or Ragnarok Online: rathena.

The cryptography algorithm used by by Bluehole in their network protocol is a
direct implementation of the freely available stream cipher Pike published 
in Ross Anderson's 1994 paper "On Fibonacci Keystream Generators".

## Credits

It's pretty hard to trace the origin of some of the achievements. So I will just
credit people without their specific contributions. Most of them did discover
specific issues while reverse engineering the TERA network protocol or did some
other kind of groundwork that this server is based on (ordered alphabetically):

alexrp, caali-hackerman, mirrawrs, meishu, P5yl0, pinkiepie
