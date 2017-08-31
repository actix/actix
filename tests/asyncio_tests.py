import asyncio
import textwrap

from aiohttp.web import Application, Response, StreamResponse, run_app


def app(worker):
    pass


async def intro(request):
    txt = textwrap.dedent("""\
        Type {url}/hello/John  {url}/simple or {url}/change_body
        in browser url bar
    """).format(url='127.0.0.1:8080')
    binary = txt.encode('utf8')
    resp = StreamResponse()
    resp.content_length = len(binary)
    resp.content_type = 'text/plain'
    await resp.prepare(request)
    resp.write(binary)
    return resp


async def simple(request):
    return Response(text="Simple answer")


async def init():
    app = Application()
    app.router.add_get('/', intro)
    app.router.add_get('/simple', simple)
    return app


# loop = asyncio.get_event_loop()
# app = loop.run_until_complete(init(loop))
# run_app(app)
