# 20: Search

**What to build:** the user types in the header and gets suggestions without leaving the Page they're on. Submitting a search or picking a suggestion opens a Search Page with tabs by type, and their last 10 searches are remembered. See user stories 42–44.

**Blocked by:** 17

**Status:** ready-for-agent

- [ ] Suggestions are debounced 300 ms and shown in a Shell-owned dropdown under the header search
- [ ] Submitting, or picking a suggestion, pushes `Search { query, tab }`. Switching tabs replaces the Back stack entry
- [ ] A 10-entry search history is kept in `Settings` and offered in the dropdown
