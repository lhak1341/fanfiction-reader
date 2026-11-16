// The app shell: one ErrorBanner and the two list screens, the navigation
// between Search, Bookmarks and Reader, and the reconcile sequences no single
// Screen owns.
//
// Architecture review candidate 2 (2026-10-07), rejected: extracting
// navigation + the reconcile sequences below into an AppShell module fails
// the deletion test — ~30 lines, one caller. Navigation state hasn't
// changed since the initial commit; reopen only if the reconcile sequences
// start growing.
import { useEffect, useState } from "react";
import { ArrowLeft, RefreshCw, X } from "lucide-react";
import {
  Button,
  EmptyState,
  Icon,
  ProgressBar,
  Tab,
  TabList,
  TabPanel,
  Tabs,
} from "@lhak/style/react";
import { BookmarksScreen } from "$lib/bookmarksScreen";
import { ErrorBanner } from "$lib/errorBanner";
import { getBookmarksHideRead } from "$lib/filterPrefs";
import { SearchScreen } from "$lib/searchScreen";
import {
  toggleBookmarkedAndRefresh,
  toggleDismissedAndRefresh,
  toggleReadAndRefresh,
} from "$lib/storyFlagActions";
import type { StoryRow } from "$lib/types";
import { getViewMode, saveViewMode, type ViewMode } from "$lib/viewMode";
import { AppearanceMenu } from "./AppearanceMenu";
import { BookmarksView } from "./BookmarksView";
import { SearchView } from "./SearchView";
import { useStore } from "./useStore";

type ListScreen = "search" | "bookmarks";

function createScreens() {
  const banner = new ErrorBanner();
  return {
    banner,
    search: new SearchScreen(banner),
    bookmarks: new BookmarksScreen(banner, getBookmarksHideRead()),
  };
}

export function App() {
  const [screens] = useState(createScreens);
  const banner = useStore(screens.banner);
  // The views subscribe to the screens they draw; App reads only the bulk job.
  const { search } = screens;
  const bookmarks = useStore(screens.bookmarks);

  const [screen, setScreen] = useState<ListScreen | "reader">("search");
  const [lastListScreen, setLastListScreen] = useState<ListScreen>("search");
  const [reading, setReading] = useState<StoryRow | undefined>(undefined);

  // Shared between Search and Bookmarks — one persisted preference, not
  // per-screen state.
  const [viewMode, setViewMode] = useState<ViewMode>(getViewMode);
  useEffect(() => saveViewMode(viewMode), [viewMode]);

  // Reload-on-filter-change and pref persistence live in the Screens'
  // `start()`; the first loads of what no filter drives happen here.
  useEffect(() => {
    search.start();
    bookmarks.start();
    void search.loadFandoms();
    void bookmarks.loadFandoms();
    void bookmarks.loadContinueReading();
    return () => {
      search.dispose();
      bookmarks.dispose();
    };
  }, [search, bookmarks]);

  // Bookmark refresh/download loops keep running behind Reader (see
  // BookmarksScreen's gate) — hold them while it's open.
  useEffect(() => {
    if (screen === "reader") bookmarks.pauseBackgroundFetches();
    else bookmarks.resumeBackgroundFetches();
  }, [screen, bookmarks]);

  function openStory(story: StoryRow) {
    if (screen !== "reader") setLastListScreen(screen);
    setReading(story);
    setScreen("reader");
  }

  // Any toggle can affect both lists (a story visible in Search may be the
  // one just bookmarked/unbookmarked) plus the bookmarked-fandoms scope and
  // the Continue Reading surface — dataset is tiny, so just refresh everything.
  async function refreshAll() {
    await Promise.all([search.loadStories(), bookmarks.reloadAll()]);
  }

  async function handleRefreshBookmarks() {
    await bookmarks.refresh();
    await search.loadStories();
  }

  async function handleDownloadAllChapters(story: StoryRow) {
    await bookmarks.downloadAllChapters(story);
    await search.loadStories();
  }

  const handlers = {
    onToggleRead: (story: StoryRow) => void toggleReadAndRefresh(story, banner, refreshAll),
    onToggleDismissed: (story: StoryRow) =>
      void toggleDismissedAndRefresh(story, banner, refreshAll),
    onToggleBookmarked: (story: StoryRow) =>
      void toggleBookmarkedAndRefresh(story, banner, refreshAll),
  };

  if (screen === "reader") {
    // The reading view moves to React with ticket 12.
    return (
      <div className="fanf-page">
        <EmptyState
          title={reading?.title ?? "Reader"}
          description="The reader isn't on the new frontend yet."
          action={
            <Button onClick={() => setScreen(lastListScreen)}>
              <Icon icon={ArrowLeft} /> Back
            </Button>
          }
        />
      </div>
    );
  }

  const refreshing = bookmarks.job?.kind === "refresh";
  const download = bookmarks.job?.kind === "download" ? bookmarks.job : undefined;
  // The kit Button sets no tooltip: the title carries the refresh progress on hover.
  const refreshLabel = refreshing
    ? bookmarks.progress.label || "Refreshing…"
    : bookmarks.busy
      ? "Wait for the current download to finish"
      : "Refresh bookmarked stories (in-progress only)";

  return (
    <Tabs
      className="fanf-page"
      value={screen}
      onValueChange={(value: ListScreen) => setScreen(value)}
    >
      <header className="fanf-top">
        <h1 className="fanf-wordmark">
          FanFiction<em>Reader</em>
        </h1>
        <TabList aria-label="Library">
          <Tab value="search">Search</Tab>
          <Tab value="bookmarks">Bookmarks</Tab>
        </TabList>
        <div className="fanf-top-right">
          <Button
            variant="ghost"
            iconOnly
            aria-label={refreshLabel}
            title={refreshLabel}
            className={refreshing ? "fanf-spinning" : undefined}
            disabled={bookmarks.busy}
            onClick={() => void handleRefreshBookmarks()}
          >
            <Icon icon={RefreshCw} />
          </Button>
          <AppearanceMenu />
        </div>
      </header>

      {banner.messages.map((m) => (
        <div key={m.key} className="fanf-error" role="alert">
          <span>{m.text}</span>
          <Button
            variant="ghost"
            iconOnly
            aria-label="Dismiss"
            onClick={() => banner.dismiss(m.key)}
          >
            <Icon icon={X} />
          </Button>
        </div>
      ))}

      {download && (
        <ProgressBar
          className="fanf-download"
          label={`Downloading "${download.title}"`}
          valueText={bookmarks.progress.label || undefined}
          value={
            bookmarks.progress.total
              ? (bookmarks.progress.done / bookmarks.progress.total) * 100
              : 0
          }
        />
      )}

      <TabPanel value="search" className="fanf-panel">
        <SearchView
          search={search}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onOpenStory={openStory}
          {...handlers}
        />
      </TabPanel>
      <TabPanel value="bookmarks" className="fanf-panel">
        <BookmarksView
          bookmarks={bookmarks}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onOpenStory={openStory}
          onDownloadAll={(story) => void handleDownloadAllChapters(story)}
          {...handlers}
        />
      </TabPanel>
    </Tabs>
  );
}
