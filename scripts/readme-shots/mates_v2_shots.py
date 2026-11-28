"""Screenshots of Evo Mates UI v2 with sample data, plus a sideways-scroll scan.

Loads Evo Mates' window (``mates.html``) from the desktop app's Vite dev
server in Playwright's Chromium with the fake Tauri IPC of ``mock-tauri.js``
and the extra states of ``mock-mates-v2.js`` (docs/features/evo-mates-ui-v2/
tasks.md, T26). Nothing here is app code; the app itself always runs against
the real engine.

Each view is taken in light and dark at each width given and written as
``<view>-<width>-<theme>.png``. ``--scan`` also lists every scroll box whose
content is wider than it (NFR-1).

Example::

    cd apps/desktop && bun x vite --port 1440 --strictPort
    python scripts/readme-shots/mates_v2_shots.py --url http://localhost:1440 --out shots
"""

from __future__ import annotations

import argparse
import re
import pathlib

from playwright.sync_api import Page, sync_playwright

HERE = pathlib.Path(__file__).resolve().parent
HEIGHT = 900

STORAGE = """
try {{
  localStorage.setItem('evo.theme', '{theme}');
  localStorage.setItem('evo.ui', JSON.stringify({{ state: {{ approvalNoteSeen: true }}, version: 0 }}));
  localStorage.setItem('evo.mates-ui', JSON.stringify({{ state: {{ sidebarFolded: {folded}, foldedSections: [], filesView: 'grid', drafts: {{ 'chat-rio': 'Can you also check the APAC numbers before Friday?' }} }}, version: 0 }}));
}} catch {{}}
"""

# Every scroll box whose content is wider than its box (qa-2026-11-28's scan).
SCAN = """() => {
  const out = [];
  for (const el of document.querySelectorAll('*')) {
    const s = getComputedStyle(el);
    if (!['auto', 'scroll'].includes(s.overflowX) && el !== document.scrollingElement) continue;
    if (el.scrollWidth > el.clientWidth + 1) {
      if (el.closest('pre, table, [data-scrolls-x]')) continue;
      out.push(`${el.tagName.toLowerCase()}.${String(el.className).slice(0, 60)} ${el.scrollWidth}>${el.clientWidth}`);
    }
  }
  return out;
}"""


def click_text(page: Page, text: str, exact: bool = True) -> None:
    page.get_by_text(text, exact=exact).first.click()
    page.wait_for_timeout(900)


def view_today(page: Page) -> None:
    page.wait_for_timeout(600)


def view_mate(page: Page) -> None:
    page.get_by_role("button", name="Ava", exact=False).first.click()
    page.wait_for_timeout(1500)


def view_mate_top(page: Page) -> None:
    view_mate(page)
    page.evaluate("() => { const s = document.querySelector('[data-thread]'); if (s) s.scrollTop = 0; }")
    page.wait_for_timeout(500)


def view_profile(page: Page) -> None:
    view_mate(page)
    page.keyboard.press("Control+i")
    page.wait_for_timeout(900)


def view_waiting(page: Page) -> None:
    page.get_by_role("button", name="Lin", exact=False).first.click()
    page.wait_for_timeout(1500)


def view_channel(page: Page) -> None:
    page.get_by_role("button", name="#Launch room", exact=False).first.click()
    page.wait_for_timeout(1500)


def view_thread(page: Page) -> None:
    view_channel(page)
    page.get_by_role("button", name=re.compile(r"\d+ repl")).first.click()
    page.wait_for_timeout(900)


def view_board(page: Page) -> None:
    view_channel(page)
    page.get_by_role("tab", name="Board").click()
    page.wait_for_timeout(900)


def view_board_list(page: Page) -> None:
    view_board(page)
    page.get_by_role("button", name="List").click()
    page.wait_for_timeout(500)


def view_card(page: Page) -> None:
    view_board(page)
    page.locator('[data-card="LAUNCH-2"]').click()
    page.wait_for_timeout(900)


def view_board_big(page: Page) -> None:
    page.evaluate("localStorage.setItem('mock.bigBoard', '1')")
    page.reload()
    page.wait_for_timeout(1500)
    started = page.evaluate("performance.now()")
    view_board(page)
    print(f"  500 cards: board shown {page.evaluate('performance.now()') - started:.0f} ms after the click (with fixed waits)")


def view_threads_tab(page: Page) -> None:
    view_channel(page)
    page.get_by_role("tab", name="Threads").click()
    page.wait_for_timeout(800)


def view_threads_page(page: Page) -> None:
    page.get_by_role("button", name=re.compile(r"^Threads")).first.click()
    page.wait_for_timeout(800)


def view_share(page: Page) -> None:
    view_channel(page)
    line = page.locator('[data-entry="k1"]')
    line.hover()
    page.get_by_role("button", name="Share to another conversation").first.click()
    page.locator('[data-entry="l1"] [data-bubble]').click()
    page.wait_for_timeout(300)
    page.get_by_role("button", name="Share…").click()
    page.wait_for_timeout(700)


def view_linked(page: Page) -> None:
    view_channel(page)
    page.get_by_role("button", name="#Launch room: more").click()
    page.get_by_role("menuitem", name="Linked conversations…").click()
    page.wait_for_timeout(700)


def view_members(page: Page) -> None:
    view_channel(page)
    page.keyboard.press("Control+i")
    page.wait_for_timeout(700)


def view_switcher(page: Page) -> None:
    page.keyboard.press("Control+k")
    page.wait_for_timeout(400)
    page.keyboard.type("a")
    page.wait_for_timeout(700)


def view_shortcuts(page: Page) -> None:
    page.keyboard.press("Control+/")
    page.wait_for_timeout(600)


def view_find(page: Page) -> None:
    view_mate(page)
    page.keyboard.press("Control+f")
    page.wait_for_timeout(300)
    page.keyboard.type("links")
    page.wait_for_timeout(700)


def view_files(page: Page) -> None:
    page.get_by_role("button", name="Files", exact=True).first.click()
    page.wait_for_timeout(900)


def view_gallery(page: Page) -> None:
    page.keyboard.press("Control+n")
    page.wait_for_timeout(900)


def view_edit(page: Page) -> None:
    view_mate(page)
    page.locator("main textarea").last.click()
    page.keyboard.press("ArrowUp")
    page.wait_for_timeout(600)


def view_slash(page: Page) -> None:
    view_mate(page)
    page.locator("main textarea").last.click()
    page.keyboard.type("/")
    page.wait_for_timeout(500)


def view_emoji(page: Page) -> None:
    view_mate(page)
    page.get_by_role("button", name="Emoji").last.click()
    page.wait_for_timeout(600)


def view_shortcode(page: Page) -> None:
    view_channel(page)
    page.locator("main textarea").last.click()
    page.keyboard.type("ship it :ro")
    page.wait_for_timeout(500)


def view_apps(page: Page) -> None:
    view_profile(page)
    page.get_by_role("tab", name="Apps").click()
    page.wait_for_timeout(700)


VIEWS = {
    "today": view_today,
    "mate": view_mate,
    "mate-top": view_mate_top,
    "profile": view_profile,
    "waiting": view_waiting,
    "channel": view_channel,
    "members": view_members,
    "thread": view_thread,
    "board": view_board,
    "threads-tab": view_threads_tab,
    "threads-page": view_threads_page,
    "share": view_share,
    "linked": view_linked,
    "board-list": view_board_list,
    "card": view_card,
    "board-big": view_board_big,
    "switcher": view_switcher,
    "shortcuts": view_shortcuts,
    "find": view_find,
    "files": view_files,
    "gallery": view_gallery,
    "edit": view_edit,
    "slash": view_slash,
    "emoji": view_emoji,
    "shortcode": view_shortcode,
    "apps": view_apps,
}


def shoot(browser, url: str, out: pathlib.Path, view: str, width: int, theme: str, folded: bool, scan: bool) -> None:
    context = browser.new_context(viewport={"width": width, "height": HEIGHT}, device_scale_factor=1, color_scheme=theme)
    context.add_init_script(path=str(HERE / "mock-tauri.js"))
    context.add_init_script(path=str(HERE / "mock-fs.js"))
    context.add_init_script(path=str(HERE / "mock-mates-v2.js"))
    context.add_init_script(STORAGE.format(theme=theme, folded="true" if folded else "false"))
    page = context.new_page()
    errors: list[str] = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.goto(url.rstrip("/") + "/mates.html", wait_until="networkidle")
    page.wait_for_timeout(1800)
    page.evaluate("document.fonts.ready")
    VIEWS[view](page)
    page.mouse.move(width - 4, HEIGHT - 4)
    page.wait_for_timeout(300)
    name = f"{view}{'-folded' if folded else ''}-{width}-{theme}.png"
    page.screenshot(path=str(out / name))
    print(f"wrote {name}")
    if scan:
        wide = page.evaluate(SCAN)
        for line in wide:
            print(f"  sideways: {line}")
    unknown = page.evaluate("[...window.__mockUnknown].sort()")
    if unknown:
        print(f"  null answers: {', '.join(unknown)}")
    for error in errors:
        print(f"  page error: {error}")
    context.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--url", default="http://localhost:1420/")
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--views", default=",".join(VIEWS))
    parser.add_argument("--widths", default="1440,1000,720")
    parser.add_argument("--themes", default="light,dark")
    parser.add_argument("--folded", action="store_true", help="start with the sidebar folded")
    parser.add_argument("--scan", action="store_true", help="list boxes that scroll sideways")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        try:
            for view in args.views.split(","):
                for width in (int(w) for w in args.widths.split(",")):
                    for theme in args.themes.split(","):
                        shoot(browser, args.url, args.out, view, width, theme, args.folded, args.scan)
        finally:
            browser.close()


if __name__ == "__main__":
    main()
