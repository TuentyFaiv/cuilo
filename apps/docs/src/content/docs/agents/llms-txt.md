---

title: Give an agent the docs
description: The docs site serves /llms.txt and /llms-full.txt, plain-text copies of the English docs that you can hand to an AI agent.
---

The docs site serves two plain-text files for AI agents. They hold the English pages only.

| File                               | What it holds                                                                                                                                    | Use it when                                                                 |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| [`/llms.txt`](/llms.txt)           | An index: each page's title, its description and a link to it, grouped by sidebar section, following the [llms.txt](https://llmstxt.org) format. | The agent can fetch pages itself and only needs to know which page to read. |
| [`/llms-full.txt`](/llms-full.txt) | The full text of every page, each with its title and source link, then the official packs with their pieces, inputs and stacks.                  | You want the agent to have everything in one file.                          |

Both are built from the same pages as the site, so they change when the docs change. The landing page is left out. In `/llms-full.txt`, import lines, self-closing components and HTML comments are removed from each page.

## Give the files to an agent

### Agents that fetch URLs

Paste the URL into the conversation and ask the agent to read it:

```text
Read https://cuilo.dev/llms.txt, then the pages you need, and use cuilo to add a Button atom.
```

Start with `/llms.txt`. It is small, and each line says what the page covers.

### Agents that read local files

Download the full text and point the agent at the file:

```sh
curl --proto '=https' -fsSL https://cuilo.dev/llms-full.txt -o cuilo-docs.txt
```

The file is a snapshot. Download it again after you upgrade cuilo, since the docs follow the code.

### Project instructions

If your agent reads an instructions file in the project, add a line that points to the index:

```md
cuilo docs for agents: https://cuilo.dev/llms.txt
```

## Docs and tools

The files explain cuilo. They do not let an agent change your project. To let an agent plan and apply pieces, connect it over MCP, as [Connect an agent over MCP](/agents/mcp/) describes. Use both: the docs tell the agent how cuilo behaves, and the tools do the work, showing you each plan first.
