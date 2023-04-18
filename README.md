Welcome to the tug early access repo! This isn't secretive or anything, I just
want to get feedback before writing blog posts and becoming a nuisance.

# Building

I mean it's just a Rust project. Get [rustup](https://rustup.rs) and run
`cargo build --release`. Your binary will appear as `target/release/tug`, and
you can put it where ever you put binaries. I won't judge.

# Getting set up

First thing's first, you'll need a podman system service socket. If you want to
target your local system and you have systemd, you can use
`systemctl enable podman --user` to enable the podman system service socket.
It'll probably be at `unix:///run/user/1000/podman/podman.sock` or something.
If you'd like to target a system that you aren't, you can use SSH UDS
forwarding. We just use the socket, so it should be fine.

You'll need to tell tug where the socket is. You've got a few options for this.
First, you could use the `TUG_SERVICE` environment variable and set it to the
socket path. You could also use the global tug config file at
`<config-dir>/tug.toml` and set the `service` field. But if you don't like the
idea of a global tug config file, you can also create a local one and point to
it with `TUG_CONFIG`. So many options!

# Basic operation

To check if tug is working, run `tug ping` and it will ping the remote podman
system service socket! If this works, you're on the right track.

The core command of tug is `tug sync`. When given a directory of config files,
`tug sync` synchronizes the expected state in said config files with the actual
state. This happens using fancy computer science stuff - for the purposes of
basic operation, it's magic. You can give it a try with some of my examples in
the examples directory!

As you use tug, you may notice that your names don't show up much in the actual
created resources. This is due to naming conflicts - you can't have multiple
resources with the same name, but we want to have those, so we use labels. You
can resolve the actual ids using `tug query` and it's subcommands. So helpful!

Once you're done with tug and want to zap all the resources currently used by
tug, you can run `tug down` and it will get rid of containers and networks. You
can get rid of dangling images using `podman system prune`. If you hate me that
much.
