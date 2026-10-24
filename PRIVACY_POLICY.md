# PADE Privacy Policy

_Last updated: October 24, 2026_

PADE is a desktop application that runs entirely on your own computer. It has
no server component, no account system, and no analytics or telemetry of any
kind.

## What PADE stores

Everything PADE keeps, it keeps **locally on your machine**:

- **Settings and preferences** - project roots, pinned/recent projects, editor
  rules, appearance, and feature toggles, stored in PADE's configuration
  directory (`%APPDATA%\pade` on Windows).
- **Session snapshots** - which agent sessions were open in a window, so a
  reload or a webview crash can restore them.
- **Workspace files** - throwaway workspaces you create live under the same
  configuration directory until you save or discard them.

None of this data ever leaves your computer through PADE itself.

## What PADE does not do

- No telemetry, analytics, crash reporting, or usage tracking.
- No accounts, sign-ins, or PADE-operated servers.
- No sale or sharing of data - PADE has nothing to sell or share.

## Third-party services you may invoke through PADE

PADE launches and orchestrates tools you have installed and configured
yourself. When you use them, **their** privacy policies apply:

- **AI coding agents** (for example Claude Code, Codex, OpenCode, Copilot
  CLI): prompts and code you submit through an agent's terminal session are
  sent to that agent's provider under your own account and their terms.
- **Copilot naming and summaries** (Windows): to name a live session, and to
  summarize it on the agents dashboard, PADE sends a short excerpt of its
  terminal transcript to Microsoft Copilot (copilot.com) as the personal
  Microsoft account Windows is signed in to. Each request appears as a chat in
  that account's Copilot history, and Microsoft's terms apply. With no such
  account, PADE asks the agent CLI instead.
- **Git remotes**: cloning, fetching, and pushing communicate with the remote
  host you chose (for example GitHub). Clone credentials are passed to your
  own `git` client for a single operation and are never stored by PADE.
- **Discord Rich Presence** (opt-in, off by default): when enabled, PADE sends
  your currently open project's name and detected language to your locally
  running Discord client, which may display them on your Discord profile.
  Turning the toggle off clears the status. Nothing is sent when Discord is
  not running or the toggle is off.
- **Update check** (on by default, Config → Updates): a released PADE reads a
  small file from PADE's GitHub repository every few hours and, when it names a
  newer version, downloads that release from GitHub. The requests carry PADE's
  version in their user agent and nothing about you or your projects; GitHub's
  privacy statement applies. Turning **Check for updates** off stops them.
- **Design tools**: the Design menu opens third-party web tools in a separate
  window; your use of those sites is governed by their own policies.

## Changes

Changes to this policy are published in this file in the PADE repository, with
the date above updated.

## Contact

Questions or concerns: open an issue at
<https://github.com/avi12/pade/issues>.
