`todo-mvc` implemented with Python, Redis, and Solid, and managed by tug. You
should be able to get a pretty good overview from just the `infra` folder,
which containers all the tug definition files.

# Before you deploy

You will need to build some stuff! Make sure you have `node` (I'm sorry) and
`pnpm` installed. Go into the `frontend` directory and run `pnpm build` to
build the website. Next, go into the `backend` directory and run
`podman build -t todomvc-backend .` to build the image. You'll need to get this
image onto the target node somehow. Good luck. For me, my development machine
is also my node, so it was fairly easily.

# Architecture

This example consists of four main parts: the frontend, the backend, Redis, and
NGINX. The Redis configuration is the same as the Redis example. The frontend
is a fairly simple Solid.js, and the backend is a fairly simple Python Flask
app running behind gunicorn. The NGINX is also pretty simple, with a whopping
16 lines. It just proxies `/api/` to the backend and all other requests to a
local directory that was injected using tug at startup.
