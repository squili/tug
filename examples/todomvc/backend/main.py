import os

from flask import Flask, request, g, jsonify, abort
from flask_cors import CORS
from flask_expects_json import expects_json
from redis import Redis

app = Flask(__name__)
CORS(app)
redis = Redis.from_url(os.environ.get(
    'REDIS_URL', 'redis://redis.local'), decode_responses=True)

TODO_SCHEMA = {
    'type': 'object',
    'properties': {
        'content': {'type': 'string'},
        'complete': {'type': 'boolean'}
    },
    'required': ['content', 'complete']
}


@app.get('/')
def get():
    ids = redis.smembers('ids')
    todos = {}
    for id in ids:
        todo = redis.hgetall(f'todo:{id}')
        if todo:
            todos[id] = {
                'content': todo['content'],
                'complete': todo['complete'] == 't'
            }
    return jsonify(todos)


@app.post('/')
@expects_json(TODO_SCHEMA)
def post():
    id = redis.incr('counter')
    transaction = redis.pipeline()
    transaction.sadd(f'ids', id)
    data = g.data
    transaction.hset(f'todo:{id}', mapping={
        'content': data['content'],
        'complete': 't' if data['complete'] else 'f'
    })
    transaction.execute()

    return jsonify({
        'id': id,
    })


@app.delete('/')
def delete():
    id = request.args.get('id')
    if id == None:
        return abort(400, 'missing id')
    amount = redis.delete(f'todo:{id}')
    if amount == 0:
        return abort(404, 'not found')

    return jsonify({})


@app.put('/')
@expects_json(TODO_SCHEMA)
def put():
    try:
        id = request.args.get('id', type=int)
    except ValueError:
        return abort(400, 'id should be numeric')
    if id == None:
        return abort(400, 'missing id')
    if int(redis.get('counter')) < id:
        return abort(404, 'todo not found')
    if not (redis.exists(f'todo:{id}')):
        return abort(404, 'todo not found')
    data = g.data
    redis.hset(f'todo:{id}', mapping={
        'content': data['content'],
        'complete': 't' if data['complete'] else 'f'
    })

    return jsonify({})


if __name__ == "__main__":
    app.run()
