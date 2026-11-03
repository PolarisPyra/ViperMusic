# Changelog style

`CHANGELOG.md` is for Viper Music's users. It is the text of each GitHub release, which the
version badge in the README links to. `scripts/changelog.sh` checks every entry against the rules
below. Each entry is a commit's `Changelog:` paragraph, word for word, so it is written for users
when the commit is.

## Each entry

- One bullet per change a user can see. Say what they see or can do now, where, and what is
  different from before.
- Write for someone who uses Viper Music but has never seen its code. Everyday words, full
  sentences, present tense.
- Name places in the app the way the user finds them: "Preferences → Connections", "the sidebar",
  "the player bar", "the queue", "a song's right-click menu".
- Say what a change is for when it isn't obvious. Not "Cache the seek bar" but "The seek bar now
  redraws only when it moves, so Viper Music uses less CPU while a song plays."
- An exception gets its own bullet in the same plain words.
- One to three sentences. No line breaks inside an entry.

## Never

- File paths, file names, function, type, command or setting key names, or anything in
  backticks.
- Pixel sizes, internal limits, or other implementation details.
- Developer words: refactor, component, state, render, hook, IPC, backend, frontend, crate, CSS.
- Credit or blame for agents, tools or libraries used to build Viper Music.
- Changes users can't notice, such as tests, builds, release tooling, or code tidying. They are left
  out, not described.

## Groups

A paragraph names its group first, as in `Changelog: Fixed: …`. A release lists the groups in
this order:

| Group | For |
| --- | --- |
| Added | Something new the user can do or see |
| Changed | Something that works or looks different |
| Fixed | Something that was broken and now works |
| Removed | Something that is gone |
| Security | A fix that protects the user's data or machine |

## One change, several commits

A release often has several commits for one change: the change, a follow-up, a fix for a bug the
change itself introduced. Nothing merges them, so give the change its entry once, describing the end
result, and give the rest `Changelog: none`. A fix for a bug that never reached a release is not a
Fixed entry.
