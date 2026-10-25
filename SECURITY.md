# Security

Delest moves and deletes files, runs a local model and installs hooks in AI agents.
If you find a way to make it delete something it should protect, to run code it should
not, or to send data it should not, please report it privately.

## How to report

Use GitHub's private reporting: **Security › Report a vulnerability** on this
repository. You can also write to kaabaoui.othman@gmail.com.

Please include the version (`delest --version`), the system, and the steps to reproduce.
You will get an answer within a week. Please do not open a public issue for a
vulnerability before it is fixed.

## What Delest promises

- Nothing is deleted without the developer's confirmation; the quarantine keeps
  everything restorable for seven days.
- Work git does not track, files tracked by git, secrets, source code and Docker volumes
  are never proposed.
- The models and the llama.cpp engine are downloaded only on the developer's request,
  each checked against its published SHA-256 fingerprint.
