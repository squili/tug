This example shows a simple Redis database managed by tug.

# Connecting to Redis

## Port forwarding

Add the following to the `container "redis"` block:
```kdl
port 6379
```
You should be able to access Redis now via typical methods like just running
`redis-cli`!

## Network

Say you don't want to expose Redis to just anyone who asks nicely. That's
pretty reasonable. After synchronizing to your node, run
`tug query network redis`. In the output you should see a network id - that's
the real network id for the "redis" tug network! You can manually run a
container connected to it with
`podman run -it --network <network-id> redis redis-cli -h redis.local`.
In that shell you can run any Redis commands you want - it will send requests
to the Redis container. Just type `exit` or press `ctrl` + `d` to leave.

# Persistence

This Redis configuration comes with persistence via both RDB and AOF - but
let's test it to make sure! First, connect to Redis using on the above methods
and run a command like `SET foo bar` to write a value to the database and
disconnect. Next, we need to restart the container. We can get the container's
id by running `tug query container redis`. With that id, we can run
`podman rm -f <container-id>` to delete it. Connect back again and run a
command like `GET foo`. You should get the original value you put in again.
