---

title: Dale la documentación a un agente
description: El sitio sirve /llms.txt y /llms-full.txt, copias en texto plano de la documentación en inglés que puedes darle a un agente de IA.
---

El sitio de documentación sirve dos archivos de texto plano para agentes de IA. Solo contienen las páginas en inglés: las páginas en español no están incluidas.

| Archivo                            | Qué contiene                                                                                                                                                | Úsalo cuando                                                                         |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [`/llms.txt`](/llms.txt)           | Un índice: el título de cada página, su descripción y un enlace, agrupados por sección de la barra lateral, con el formato [llms.txt](https://llmstxt.org). | El agente puede descargar las páginas por su cuenta y solo necesita saber cuál leer. |
| [`/llms-full.txt`](/llms-full.txt) | El texto completo de cada página, cada una con su título y el enlace a su origen, y después los packs oficiales con sus piezas, entradas y stacks.          | Quieres que el agente tenga todo en un solo archivo.                                 |

Los dos se generan a partir de las mismas páginas que el sitio, así que cambian cuando cambia la documentación. La página de inicio queda fuera. En `/llms-full.txt` se quitan de cada página las líneas de import, los componentes que se cierran solos y los comentarios HTML.

## Dale los archivos a un agente

### Agentes que descargan URLs

Pega la URL en la conversación y pídele al agente que la lea:

```text
Read https://cuilo.dev/llms.txt, then the pages you need, and use cuilo to add a Button atom.
```

Empieza con `/llms.txt`. Es pequeño, y cada línea dice qué cubre la página.

### Agentes que leen archivos locales

Descarga el texto completo y apunta al agente a ese archivo:

```sh
curl --proto '=https' -fsSL https://cuilo.dev/llms-full.txt -o cuilo-docs.txt
```

El archivo es una copia de ese momento. Descárgalo otra vez después de actualizar cuilo, porque la documentación sigue al código.

### Instrucciones del proyecto

Si tu agente lee un archivo de instrucciones en el proyecto, agrega una línea que apunte al índice:

```md
cuilo docs for agents: https://cuilo.dev/llms.txt
```

## Documentación y herramientas

Los archivos explican cuilo. No le permiten a un agente cambiar tu proyecto. Para que un agente planee y aplique piezas, conéctalo por MCP, como explica [Conecta un agente por MCP](/es-mx/agents/mcp/). Usa los dos: la documentación le dice al agente cómo se comporta cuilo, y las herramientas hacen el trabajo y te muestran cada plan primero.
