# Session-wide terminal transformation

The tool must support opt-in reading sessions in addition to explicit text and stdin filtering. Session-wide integration will use a pseudo-terminal so child programs retain terminal behavior; shell configuration and framework adapters will activate that shared session mechanism. Redirecting a shell's output through an ordinary pipe changes terminal detection, while explicit command wrappers alone do not satisfy the agreed session-wide requirement. This choice makes terminal resizing, signals, job control, and interactive-program compatibility part of the session implementation.

Reading sessions emphasize command output while preserving shell prompts, command-line editing, and typed input through shell integration boundaries. Known full-screen and interactive applications use passthrough; session-wide support does not mean blindly transforming every byte from the terminal.

The compatibility contract covers shell prompts and editing, bypasses known interactive commands, and falls back to passthrough when terminal behavior is uncertain. A pseudo-terminal does not provide reliable attribution for application-generated prompts, nested interactive programs, or background output. Configurable command exclusions and an explicit bypass are required; arbitrary application behavior is not guaranteed to be detectable in advance.

This approach follows the byte-stream behavior described by the [PTY interface](https://man7.org/linux/man-pages/man7/pty.7.html) and the execution boundaries illustrated by [terminal shell-integration markers](https://iterm2.com/documentation-escape-codes.html#Shell_IntegrationFinalTerm).
