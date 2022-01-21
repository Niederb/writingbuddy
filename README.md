# writingbuddy

![Rust](https://github.com/Niederb/writingbuddy/workflows/build/badge.svg)

Simple command line tool to help you establish a daily writing habit.

## Features

- Uses simple markdown files to store text
- 'Strict mode' that does not let you leave before you reach your defined goals
- A keystroke timer that forces you to keep on writing
- Append text to existing markdown file
- Define your own title and filename patterns
- Option to disable backspace key in order to focus on writing
- Define a goal of how many words you want to write
- Define a goal of how long you want to write

```
  ┌Title─────────────────────────────────────────────────────────┐
  │2022-01-19                                                    │
  └──────────────────────────────────────────────────────────────┘
  ┌Text──────────────────────────────────────────────────────────┐
  │Hello World.                                                  │
  │                                                              │
  │                                                              │
  │                                                              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
  ┌Word count────────────────────┐┌Time──────────────────────────┐
  │2/10                          ││270 s                         │
  └──────────────────────────────┘└──────────────────────────────┘

```