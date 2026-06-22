import { Marked } from 'marked';
import mermaid from 'mermaid';

// Initialize mermaid
mermaid.initialize({
  startOnLoad: false,
  theme: 'dark',
  securityLevel: 'loose'
});

const markedInstance = new Marked();

// Custom renderer to intercept mermaid code blocks
const renderer = {
  code({ text, lang }) {
    if (lang === 'mermaid' || lang === 'mmd') {
      // Return a div with class 'mermaid' so mermaid.run() can pick it up
      return `<div class="mermaid" style="text-align: center; margin: 20px 0;">${text}</div>`;
    }
    return false; // let marked use default rendering for other code blocks
  }
};

markedInstance.use({ renderer });

export function renderMarkdown(text) {
  // Add some basic styling classes to the parsed HTML
  let html = markedInstance.parse(text);
  // Optional: You could do minor string replacements here if needed
  // e.g. html = html.replace(/<ul>/g, '<ul style="padding-left: 20px;">');
  return html;
}

export async function runMermaid() {
  try {
    // Run mermaid on all elements with class 'mermaid'
    await mermaid.run({
      querySelector: '.mermaid',
      suppressErrors: true
    });
  } catch (err) {
    console.warn("Mermaid rendering error:", err);
  }
}
