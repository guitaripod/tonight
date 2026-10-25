#!/usr/bin/env python3
"""Regenerates docs/screens/*.png from a running instance.

    cargo run --release -- -p 7788 &
    scripts/screens.py http://localhost:7788

Needs `pip install playwright` and `playwright install chromium`. Dealing a hand
writes to the database, so point this at a copy of your data, not the real one.
"""
import pathlib
import sys

from playwright.sync_api import sync_playwright

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:7777"
OUT = pathlib.Path(__file__).resolve().parent.parent / "docs" / "screens"


def settle(page):
    try:
        page.wait_for_function("[...document.images].every(i => i.complete)", timeout=20000)
    except Exception:
        pass
    page.wait_for_timeout(400)


def main():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_context(viewport={"width": 1280, "height": 1180}).new_page()
        page.goto(BASE + "/#/")
        page.evaluate("localStorage.clear(); sessionStorage.clear()")
        page.goto(BASE + "/#/")
        page.wait_for_selector(".ritual")
        page.click(".choice[data-value='90']")
        page.click(".toggle[data-id=quiet]")
        page.click(".toggle[data-id=interrupt]")
        page.wait_for_function("document.querySelector('.fit-meter')?.textContent.includes('fit tonight')")
        page.wait_for_timeout(300)
        page.screenshot(path=str(OUT / "table.png"))

        page.set_viewport_size({"width": 1280, "height": 860})
        page.click(".deal-bar .btn")
        page.wait_for_selector(".card.revealed", timeout=10000)
        page.wait_for_timeout(1400)
        settle(page)
        page.screenshot(path=str(OUT / "hand.png"))

        page.set_viewport_size({"width": 1280, "height": 1500})

        page.click(".seat >> nth=0 >> text=Why this?")
        page.wait_for_selector(".fit-list", timeout=10000)
        page.wait_for_timeout(500)
        page.locator("#sheet").screenshot(path=str(OUT / "why.png"))
        browser.close()


main()
