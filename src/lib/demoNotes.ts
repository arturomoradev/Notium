import { call } from "./ipc/commands";
import type { NoteSummaryDto } from "./ipc/types";

export const DEMO_GUIDE_TITLE = "Guía Visual y Ejemplos de Notium";
export const LOREM_1_TITLE = "Lorem Ipsum - Parte 1";
export const LOREM_2_TITLE = "Lorem Ipsum - Parte 2";

const GUIDE_BODY = `# Guía Visual y Ejemplos de Notium

Esta nota sirve como **catálogo visual y banco de pruebas** de todas las funciones de renderizado en vivo (*Live Preview*), marcaciones Markdown, Callouts, fórmulas matemáticas, diagramas y enlaces de secciones de **Notium**.

Etiquetas de prueba: #guia #markdown #notium #ejemplos

## Índice Rápido (Enlaces a Secciones Internas)

Haz clic en cualquiera de estos enlaces de sección para desplazarte automáticamente:
- [[#1. Tipografía y Formato Inline]]
- [[#2. Listas, Tareas y Citas]]
- [[#3. Tablas y Bloques de Código]]
- [[#4. Callouts de Notium]]
- [[#5. Matemáticas LaTeX]]
- [[#6. Diagramas Mermaid]]
- [[#7. Enlaces Cruzados y Transclusión]]

---

## 1. Tipografía y Formato Inline

Ejemplos de estilos de texto disponibles:
- **Texto en negrita** usando \`**doble asterisco**\`
- *Texto en cursiva* usando \`*un asterisco*\`
- ~~Texto tachado~~ usando \`~~doble virgulilla~~\`
- ==Texto resaltado== usando \`==doble igual==\`
- \`Código en línea (inline code)\` usando comillas invertidas
- Enlace web externo: [Repositorio y Documentación](https://github.com)

Volver arriba: [[#Índice Rápido (Enlaces a Secciones Internas)]]

---

## 2. Listas, Tareas y Citas

### Lista de viñetas
- Primer elemento de lista
- Segundo elemento con **énfasis** y \`código\`
  - Subelemento indentado con \`Tab\`

### Lista numerada
1. Paso inicial de configuración
2. Creación y organización de notas
3. Vinculación mediante \`[[Wikilinks]]\`

### Lista de tareas interactiva (Checklist)
- [x] Renderizado en vivo de Markdown
- [x] Soporte de Callouts, Matemáticas y Mermaid
- [x] Enlaces a secciones \`[[#Sección]]\` y \`[[Nota#Sección]]\`
- [ ] Probar marcando o desmarcando esta casilla directamente

### Cita estándar (Blockquote)
> La simplicidad es la máxima sofisticación. Tus notas siempre permanecen en archivos Markdown locales bajo tu control.

---

## 3. Tablas y Bloques de Código

### Tabla Markdown renderizada
| Elemento | Sintaxis | Renderizado en Vivo |
| --- | --- | --- |
| Negrita | \`**texto**\` | Sí |
| Sección interna | \`[[#Sección]]\` | Sí |
| Sección externa | \`[[Nota#Sección]]\` | Sí |
| Incrustación | \`![[Nota#Sección]]\` | Sí |
| Matemáticas | \`$E = mc^2$\` | Sí |

### Bloque de código con etiqueta de lenguaje y botón Copiar
\`\`\`rust
pub fn saludo(nombre: &str) -> String {
    format!("¡Hola desde Notium, {nombre}!")
}
\`\`\`

---

## 4. Callouts de Notium

A continuación se muestran **todos los tipos de Callouts** soportados con su respectivo nombre e identificador:

> [!note] Callout de Nota (\`[!note]\`)
> Ideal para apuntes generales, aclaraciones o contexto adicional dentro de un documento.

> [!info] Callout de Información (\`[!info]\`)
> Útil para destacar datos objetivos, referencias o estado del documento.

> [!tip] Callout de Consejo / Sugerencia (\`[!tip]\` o \`[!hint]\`)
> Úsalo para atajos, buenas prácticas o trucos que ahorran tiempo.

> [!important] Callout Importante (\`[!important]\`)
> Destaca requisitos esenciales o puntos clave que no deben pasarse por alto.

> [!success] Callout de Éxito / Verificado (\`[!success]\` o \`[!check]\`)
> Confirma que un procedimiento, prueba o validación se completó correctamente.

> [!warning] Callout de Advertencia (\`[!warning]\` o \`[!caution]\`)
> Advierte sobre posibles precauciones o cambios delicados.

> [!danger] Callout de Peligro / Error (\`[!danger]\`, \`[!error]\` o \`[!bug]\`)
> Señala acciones críticas, errores conocidos o riesgos de pérdida de datos.

> [!question] Callout de Pregunta / FAQ (\`[!question]\` o \`[!faq]\`)
> Perfecto para plantear dudas abiertas o preguntas frecuentes.

> [!example] Callout de Ejemplo (\`[!example]\`)
> Muestra casos prácticos o demostraciones paso a paso.

> [!quote] Callout de Cita (\`[!quote]\`)
> Resalta citas textuales de libros, artículos o autores.

> [!note]- Callout Plegable (\`[!note]-\`)
> Añadiendo un signo \`-\` después del tipo (\`> [!note]-\`), el callout se puede plegar y desplegar haciendo clic en su encabezado.

---

## 5. Matemáticas LaTeX

### Fórmulas en línea (Inline Math)
La equivalencia masa-energía es $E = mc^2$, la identidad de Euler relaciona $\\pi$ y $e$, y la solución cuadrática es $x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}$.

### Bloques matemáticos (\`$$ ... $$\` o \`\`\`math)
$$
\\sum_{i=1}^{n} i^2 = \\frac{n(n+1)(2n+1)}{6}
$$

\`\`\`math
\\int_{0}^{\\infty} e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}
\`\`\`

---

## 6. Diagramas Mermaid

### Diagrama de Flujo (\`flowchart LR\`)
\`\`\`mermaid
flowchart LR
  A["Nota Markdown"] --> B["Extractor Rust"]
  B --> C["Índice SQLite FTS5"]
  C --> D["Vista en Vivo"]
\`\`\`

### Diagrama de Secuencia (\`sequenceDiagram\`)
\`\`\`mermaid
sequenceDiagram
  Usuario->>Editor: Pulsa [[#Sección]]
  Editor->>Motor: scrollToHeading()
  Motor->>Usuario: Desplaza a la sección
\`\`\`

---

## 7. Enlaces Cruzados y Transclusión

### Enlaces a secciones de las notas de prueba (Lorem Ipsum)
- Ir a [[Lorem Ipsum - Parte 1#Introducción]]
- Ir a [[Lorem Ipsum - Parte 1#Arquitectura del Sistema]]
- Ir a [[Lorem Ipsum - Parte 1#Conclusiones]]
- Ir a [[Lorem Ipsum - Parte 2#Metodología Experimental]]
- Ir a [[Lorem Ipsum - Parte 2#Resultados Obtenidos]]

### Transclusión en vivo (\`![[Nota#Sección]]\`)
 Debajo se incrusta directamente la sección **Resumen Ejecutivo** de la nota **Lorem Ipsum - Parte 2**:

![[Lorem Ipsum - Parte 2#Resumen Ejecutivo]]

Volver al inicio: [[#Índice Rápido (Enlaces a Secciones Internas)]]
`;

const LOREM_1_BODY = `# Lorem Ipsum - Parte 1

Nota de prueba con texto extenso (*Lorem ipsum*) para verificar la navegación fluida entre secciones internas (\`[[#Sección]]\`) y secciones de otras notas (\`[[Lorem Ipsum - Parte 2#Sección]]\`) tanto en PC como en móvil.

Etiquetas: #lorem #prueba #secciones

## Índice de Secciones

1. [[#Introducción]]
2. [[#Marco Teórico]]
3. [[#Arquitectura del Sistema]]
4. [[#Análisis de Rendimiento]]
5. [[#Conclusiones]]

Enlaces directos a **Lorem Ipsum - Parte 2**:
- [[Lorem Ipsum - Parte 2#Resumen Ejecutivo]]
- [[Lorem Ipsum - Parte 2#Metodología Experimental]]
- [[Lorem Ipsum - Parte 2#Resultados Obtenidos]]
- [[Lorem Ipsum - Parte 2#Discusión Técnica]]

---

## Introducción

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

Curabitur pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius, turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis sollicitudin mauris. Integer in mauris eu nibh euismod gravida.

Ir a la siguiente sección: [[#Marco Teórico]] · Ir al final: [[#Conclusiones]] · Volver a [[#Índice de Secciones]]

---

## Marco Teórico

 Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac turpis egestas. Vestibulum tortor quam, feugiat vitae, ultricies eget, tempor sit amet, ante. Donec eu libero sit amet quam egestas semper. Aenean ultricies mi vitae est. Mauris placerat eleifend leo.

Quisque sit amet est et sapien ullamcorper pharetra. Vestibulum erat wisi, condimentum sed, commodo vitae, ornare sit amet, wisi. Aenean fermentum, elit eget tincidunt condimentum, eros ipsum rutrum orci, sagittis tempus lacus enim ac dui.

> [!info] Referencia cruzada
> Puedes contrastar este marco teórico con [[Lorem Ipsum - Parte 2#Metodología Experimental]].

Ir a [[#Arquitectura del Sistema]] · Volver a [[#Índice de Secciones]]

---

## Arquitectura del Sistema

Donec non enim in turpis pulvinar facilisis. Ut felis. Praesent dapibus, neque id cursus faucibus, tortor neque egestas augue, eu vulputate magna eros eu erat. Aliquam erat volutpat. Nam dui mi, tincidunt quis, accumsan porttitor, facilisis luctus, metus.

Phasellus ultrices nulla quis nibh. Quisque a lectus. Donec consectetuer ligula vulputate sem tristique cursus. Nam nulla quam, gravida non, commodo a, sodales sit amet, nisi. Pellentesque fermentum dolor. Aliquam quam lectus, facilisis auctor, ultrices ut, elementum vulputate, nunc.

Sed adipiscing ornare risus. Morbi est est, blandit sit amet, sagittis vel, euismod vel, velit. Pellentesque egestas sem. Suspendisse commodo ullamcorper magna.

Ir a [[#Análisis de Rendimiento]] · Ver también [[Lorem Ipsum - Parte 2#Resultados Obtenidos]] · Volver a [[#Índice de Secciones]]

---

## Análisis de Rendimiento

Morbi in sem quis dui placerat ornare. Pellentesque odio nisi, euismod in, pharetra a, ultricies in, diam. Sed arcu. Cras consequat. Praesent dapibus, neque id cursus faucibus, tortor neque egestas augue, eu vulputate magna eros eu erat.

Fusce lacinia arcu et nulla. Nulla vitae mauris non felis mollis faucibus. Lorem ipsum dolor sit amet, consectetuer adipiscing elit. Donec odio. Quisque volutpat mattis eros. Nullam malesuada erat ut turpis. Suspendisse urna nibh, viverra non, semper suscipit, posuere a, pede.

Ir a [[#Conclusiones]] · Volver a [[#Índice de Secciones]]

---

## Conclusiones

Donec nec justo eget felis facilisis fermentum. Aliquam porttitor mauris sit amet orci. Aenean dignissim pellentesque felis. Morbi in sem quis dui placerat ornare. Pellentesque odio nisi, euismod in, pharetra a, ultricies in, diam.

- Saltar al principio de esta nota: [[#Índice de Secciones]]
- Saltar a [[#Introducción]]
- Continuar leyendo en [[Lorem Ipsum - Parte 2#Discusión Técnica]]
- Volver a [[Guía Visual y Ejemplos de Notium#7. Enlaces Cruzados y Transclusión]]
`;

const LOREM_2_BODY = `# Lorem Ipsum - Parte 2

Segunda nota de prueba con *Lorem ipsum* y múltiples secciones para comprobar los enlaces de sección en ambos sentidos y la transclusión.

Etiquetas: #lorem #prueba #resultados

## Índice de Secciones

1. [[#Resumen Ejecutivo]]
2. [[#Metodología Experimental]]
3. [[#Resultados Obtenidos]]
4. [[#Discusión Técnica]]
5. [[#Referencias Cruzadas]]

---

## Resumen Ejecutivo

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Vivamus luctus urna sed urna ultricies ac tempor dui sagittis. In condimentum facilisis porta. Sed nec diam eu diam mattis viverra. Nulla fringilla, orci ac euismod semper, magna diam porttitor mauris, quis sollicitudin sapien justo in libero.

---

## Metodología Experimental

Vestibulum mollis mauris enim. Morbi euismod magna ac lorem rutrum elementum. Donec viverra auctor lobortis. Pellentesque eu est a nulla placerat dignissim. Morbi a enim in magna semper bibendum. Etiam scelerisque, nunc ac egestas consequat, odio nibh euismod nulla, eget auctor orci nibh vel nisi.

 Aliquam erat volutpat. Mauris vel neque sit amet nunc gravida congue sed sit amet purus. Quisque lacus quam, egestas ac tincidunt a, lacinia vel velit. Aenean facilisis nulla vitae urna tincidunt congue sed ut dui.

Ir a [[#Resultados Obtenidos]] · Consultar [[Lorem Ipsum - Parte 1#Marco Teórico]] · Volver a [[#Índice de Secciones]]

---

## Resultados Obtenidos

Morbiaculis,踐 mauris non, mollis, elit. Curabitur et ligula. Ut molestie a, ultricies porta urna. Vestibulum commodo volutpat a, convallis ac, laoreet enim. Phasellus fermentum in, dolor. Pellentesque facilisis. Nulla imperdiet sit amet magna.

Vestibulum dapibus, mauris nec malesuada fames ac turpis velit, rhoncus eu, luctus et interdum adipiscing wisi. Aliquam erat ac ipsum. Integer aliquam purus. Quisque lorem tortor fringilla sed, vestibulum id, eleifend justo vel bibendum sapien massa ac turpis.

Ir a [[#Discusión Técnica]] · Comparar con [[Lorem Ipsum - Parte 1#Análisis de Rendimiento]] · Volver a [[#Índice de Secciones]]

---

## Discusión Técnica

Temporibus autem quibusdam et aut officiis debitis aut rerum necessitatibus saepe eveniet ut et voluptates repudiandae sint et molestiae non recusandae. Itaque earum rerum hic tenetur a sapiente delectus, ut aut reiciendis voluptatibus maiores alias consequatur aut perferendis doloribus asperiores repellat.

Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo.

Ir a [[#Referencias Cruzadas]] · Volver a [[#Índice de Secciones]]

---

## Referencias Cruzadas

- [[#Resumen Ejecutivo]]
- [[#Metodología Experimental]]
- [[Lorem Ipsum - Parte 1#Introducción]]
- [[Lorem Ipsum - Parte 1#Arquitectura del Sistema]]
- [[Lorem Ipsum - Parte 1#Conclusiones]]
- [[Guía Visual y Ejemplos de Notium#4. Callouts de Notium]]
`;

async function upsertNoteByTitle(
  existing: NoteSummaryDto[],
  title: string,
  body: string,
  overwrite: boolean,
): Promise<string> {
  const found = existing.find((n) => n.title.toLowerCase() === title.toLowerCase());
  if (found) {
    if (overwrite) {
      const read = await call("note_read", { noteId: found.note_id });
      await call("note_save", {
        noteId: found.note_id,
        content: body,
        baseRev: read.rev,
      });
    }
    return found.note_id;
  }
  const created = await call("note_create", {
    kind: "markdown",
    title,
    folder: null,
  });
  await call("note_save", {
    noteId: created.note.note_id,
    content: body,
    baseRev: created.rev,
  });
  return created.note.note_id;
}

export async function ensureDemoNotes(
  existing: NoteSummaryDto[],
  overwrite = false,
): Promise<string> {
  await upsertNoteByTitle(existing, LOREM_2_TITLE, LOREM_2_BODY, overwrite);
  await upsertNoteByTitle(existing, LOREM_1_TITLE, LOREM_1_BODY, overwrite);
  const guideId = await upsertNoteByTitle(existing, DEMO_GUIDE_TITLE, GUIDE_BODY, overwrite);
  return guideId;
}
