# 15: Home Page

**What to build:** Home works the way TIDAL suggests music: feed tabs, sections of cards, more loaded as the user scrolls, and a refresh when the user comes back to the window after a while. See user stories 30, 40–41.

**Blocked by:** 14

**Status:** ready-for-agent

- [ ] Home shows its feed tabs and sections with cards (covers are placeholders until ticket 16)
- [ ] Tabs are part of the route: switching one replaces the Back stack entry instead of adding a step, and Back restores the tab
- [ ] More sections load as the user scrolls
- [ ] Home refreshes when the window regains focus, at most every 5 minutes
- [ ] Clicking a card navigates to its route with a `Preview` (title, cover, artist) for the Page to draw straight away
