// The reading view: progress bar, the chapter in a plain reading column on the
// dotted page, corner controls that fade in on hover (wayfinder ticket #5,
// carried over from Reader.svelte). ReaderScreen owns the loading, prefetch
// and position bookkeeping; this component hands it plain scrollTop numbers
// and applies the restore to the real scroll node.
//
// Type comes from the @lhak/style reading tokens with the reader's saved
// settings on top (`readerTypography`); chapter headings take the flavor's
// heading colours and the token heading sizes, scaled with the reading size.
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Check, ChevronLeft, ChevronRight, Menu, Star, X } from "lucide-react";
import { Button, Chip, Drawer, DrawerPopup, DrawerTrigger, Icon, Spinner } from "@lhak/style/react";
import { TYPE } from "@lhak/style";
import { chapterHtml } from "$lib/chapterHtml";
import {
  getReaderPrefs,
  readerTypography,
  saveReaderPrefs,
  type ReaderPrefs,
} from "$lib/readerPrefs";
import { ReaderScreen } from "$lib/readerScreen";
import { AppearanceMenu } from "./AppearanceMenu";
import { ReaderSettings } from "./ReaderSettings";
import { useStore } from "./useStore";

// The nearby-chapters strip shows the current chapter plus as many on either
// side as fit the window's height, not the full list (that's the drawer).
// The strip is vertically centred, so NAV_VERTICAL_MARGIN keeps it clear of
// the corner buttons and the action buttons.
const NAV_ROW_HEIGHT = 20;
const NAV_VERTICAL_MARGIN = 220;

// Arrow keys inside these move a caret or a choice, not the chapter.
const OWNS_ARROWS =
  "input, textarea, select, [contenteditable], [role='dialog'], [role='radiogroup'], [role='combobox'], [role='listbox'], [role='menu']";

export function Reader({
  storyId,
  initialChapterNum,
  onBack,
  onChanged,
}: {
  storyId: number;
  initialChapterNum: number;
  onBack: () => void;
  onChanged?: () => void;
}) {
  // One Screen per mount: App remounts the Reader for each story it opens.
  const [readerScreen] = useState(() => new ReaderScreen(storyId, onChanged));
  const screen = useStore(readerScreen);
  const scrollRef = useRef<HTMLElement>(null);
  const [scrollPct, setScrollPct] = useState(0);
  const [prefs, setPrefs] = useState<ReaderPrefs>(getReaderPrefs);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [viewportHeight, setViewportHeight] = useState(() => window.innerHeight);
  // Bumped once a load settles: the layout effect below then restores the
  // position against the committed chapter.
  const [restore, setRestore] = useState<{ num: number } | undefined>(undefined);

  function updatePrefs(patch: Partial<ReaderPrefs>) {
    setPrefs((p) => {
      const next = { ...p, ...patch };
      saveReaderPrefs(next);
      return next;
    });
  }

  function handleScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const max = el.scrollHeight - el.clientHeight;
    const pct = max <= 0 ? 100 : Math.round((el.scrollTop / max) * 100);
    setScrollPct(pct);
    readerScreen.reportScroll(pct);
  }

  useEffect(() => {
    void readerScreen
      .init(initialChapterNum)
      .then(() => setRestore({ num: readerScreen.currentChapterNum }));
    const onHidden = () => document.hidden && void readerScreen.flush();
    const onResize = () => setViewportHeight(window.innerHeight);
    document.addEventListener("visibilitychange", onHidden);
    window.addEventListener("resize", onResize);
    return () => {
      document.removeEventListener("visibilitychange", onHidden);
      window.removeEventListener("resize", onResize);
      readerScreen.dispose();
    };
    // Mount-only: the Screen is built once per story.
  }, [readerScreen]);

  // After the chapter's HTML is committed, before paint: this session's exact
  // pixel position if there is one, else the saved percentage for the chapter
  // you last read (the top for any other).
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!restore || !el) return;
    const max = el.scrollHeight - el.clientHeight;
    el.scrollTop =
      readerScreen.scrollPositions.get(restore.num) ??
      (max > 0 ? (readerScreen.resumePctFor(restore.num) / 100) * max : 0);
    el.focus({ preventScroll: true });
    handleScroll();
  }, [restore, readerScreen]);

  async function goToChapter(num: number) {
    setDrawerOpen(false);
    await readerScreen.goToChapter(num, scrollRef.current?.scrollTop);
    setRestore({ num: readerScreen.currentChapterNum });
  }

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.target instanceof Element && e.target.closest(OWNS_ARROWS)) return;
      if (e.key === "ArrowLeft") void goToChapter(readerScreen.currentChapterNum - 1);
      if (e.key === "ArrowRight") void goToChapter(readerScreen.currentChapterNum + 1);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  const html = useMemo(
    () =>
      screen.content ? chapterHtml(screen.content.content, { smartQuotes: prefs.smartQuotes }) : "",
    [screen.content, prefs.smartQuotes]
  );

  const t = readerTypography(prefs);
  const proseStyle = {
    fontFamily: t.fontFamily,
    fontSize: `${t.fontSize}px`,
    fontWeight: t.fontWeight,
    lineHeight: t.lineHeight,
    letterSpacing: `${t.letterSpacing}em`,
    "--fanf-italic-family": t.italicFamily,
    "--fanf-bold-weight": t.fontWeightBold,
  } as CSSProperties;
  // The heading-size tokens are declared at the root against the contract
  // reading size, so they scale by the reader's size over it.
  const columnStyle = {
    maxWidth: `${t.pageWidth}ch`,
    "--fanf-reading-scale": t.fontSize / TYPE.reading.size,
  } as CSSProperties;

  const current = screen.currentChapterNum;
  const story = screen.story;
  const navRadius = Math.max(
    2,
    Math.floor(
      (Math.max(1, Math.floor((viewportHeight - NAV_VERTICAL_MARGIN) / NAV_ROW_HEIGHT)) - 1) / 2
    )
  );
  const centre = screen.chapters.findIndex((c) => c.chapterNum === current);
  const nearby =
    centre === -1
      ? []
      : screen.chapters.slice(Math.max(0, centre - navRadius), centre + navRadius + 1);

  return (
    <div className="fanf-page fanf-reader">
      <div className="fanf-reader-progress" aria-hidden="true">
        <div style={{ width: `${scrollPct}%` }} />
      </div>

      <nav className="fanf-reader-nav" aria-label="Nearby chapters">
        {nearby.map((c) => (
          <button
            key={c.chapterNum}
            type="button"
            className="fanf-reader-nav-row"
            aria-current={c.chapterNum === current || undefined}
            // Fades the further a chapter sits from the current one; hover restores it.
            style={
              {
                "--fanf-nav-opacity": Math.max(0.25, 1 - Math.abs(c.chapterNum - current) * 0.16),
              } as CSSProperties
            }
            onClick={() => void goToChapter(c.chapterNum)}
          >
            <span className="fanf-reader-nav-dot" />
            <span className="fanf-reader-nav-label">
              {c.chapterNum}. {c.title}
            </span>
          </button>
        ))}
      </nav>

      <div className="fanf-reader-corner fanf-reader-corner--left">
        <Button variant="ghost" onClick={onBack}>
          <Icon icon={ChevronLeft} /> Back
        </Button>
        <Drawer side="left" open={drawerOpen} onOpenChange={setDrawerOpen}>
          <DrawerTrigger render={<Button variant="ghost" iconOnly aria-label="Chapter list" />}>
            <Icon icon={Menu} />
          </DrawerTrigger>
          <DrawerPopup
            title={story?.title ?? ""}
            description={story ? `by ${story.author}` : undefined}
            className="fanf-reader-drawer"
          >
            <ol className="fanf-reader-toc">
              {screen.chapters.map((c) => (
                <li key={c.chapterNum}>
                  <button
                    type="button"
                    aria-current={c.chapterNum === current || undefined}
                    onClick={() => void goToChapter(c.chapterNum)}
                  >
                    <span className="fanf-reader-toc-num">{c.chapterNum}</span>
                    <span>{c.title}</span>
                    {!c.isCached && (
                      <span className="fanf-reader-toc-uncached" title="Not fetched yet">
                        ○
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ol>
          </DrawerPopup>
        </Drawer>
        <ReaderSettings prefs={prefs} onChange={updatePrefs} />
      </div>
      <div className="fanf-reader-corner fanf-reader-corner--right">
        <AppearanceMenu />
      </div>

      <button
        type="button"
        className="fanf-reader-edge fanf-reader-edge--left"
        aria-label="Previous chapter"
        disabled={current <= 1}
        onClick={() => void goToChapter(current - 1)}
      >
        <Icon icon={ChevronLeft} />
      </button>
      <button
        type="button"
        className="fanf-reader-edge fanf-reader-edge--right"
        aria-label="Next chapter"
        disabled={!story || current >= story.chapterCount}
        onClick={() => void goToChapter(current + 1)}
      >
        <Icon icon={ChevronRight} />
      </button>

      <main ref={scrollRef} className="fanf-reader-scroll" tabIndex={-1} onScroll={handleScroll}>
        <article className="fanf-reader-column" style={columnStyle}>
          {story && (
            <Chip className="fanf-reader-chip">{`Chapter ${current} of ${story.chapterCount}`}</Chip>
          )}
          {screen.loading ? (
            <p className="fanf-reader-status">
              <Spinner /> Loading…
            </p>
          ) : screen.fetchError ? (
            <p className="fanf-reader-status fanf-error-text" role="alert">
              {screen.fetchError}
            </p>
          ) : screen.content ? (
            <>
              <h1
                className="fanf-chapter-title"
                style={{ fontFamily: t.fontFamily, letterSpacing: `${t.letterSpacing}em` }}
              >
                {screen.content.title}
              </h1>
              <div
                className="fanf-prose"
                style={proseStyle}
                dangerouslySetInnerHTML={{ __html: html }}
              />
            </>
          ) : null}
        </article>
      </main>

      {story && (
        <div className="fanf-reader-actions">
          {screen.toggleError && (
            <p className="fanf-error-text fanf-reader-action-error" role="alert">
              {screen.toggleError}
            </p>
          )}
          <Button
            iconOnly
            aria-label="Bookmark"
            aria-pressed={story.bookmarked}
            onClick={() => void readerScreen.toggleBookmark()}
          >
            <Icon icon={Star} />
          </Button>
          <Button
            iconOnly
            aria-label="Dismiss"
            aria-pressed={story.dismissed}
            onClick={() => void readerScreen.toggleDismiss()}
          >
            <Icon icon={X} />
          </Button>
          <Button
            iconOnly
            variant={story.isRead ? "primary" : "secondary"}
            aria-label="Mark as read"
            aria-pressed={story.isRead}
            onClick={() => void readerScreen.toggleRead()}
          >
            <Icon icon={Check} />
          </Button>
        </div>
      )}
    </div>
  );
}
