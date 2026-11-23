import asyncio
from clocks import sleep

def test_fake(clock):
    clock.sleep(1)
    sleep(1)

async def test_async_scheduler():
    await asyncio.sleep(0)
