# Almetica

This is a server for the MMORPG TERA written in rust.

## FAQ

### What are your goals?

Provide a server platform for TERA that gives better performance then the original
server while providing the same feature set.

I don't strive to emulate the original TERA server 100% the same. We will
optimize / improve functionality where it seems logical (for example stricter
validation of client commands).

This server should also act as a way to preserve TERA for the future.

### Why didn't you extend already existing server projects:

I had three requirements for the server projects for me to consider continue
developing them:

 * Open Source License
 * Some kind of tests (unit / integration test etc.)
 * Written in a compiled and typed language
 * Written with a clear design goal

None of he evaluated existing server projects did fullfil these requirements.

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
