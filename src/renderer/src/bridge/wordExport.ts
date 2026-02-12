import MarkdownIt from 'markdown-it'
import { Document, HeadingLevel, Packer, Paragraph, TextRun } from 'docx'

const md = new MarkdownIt()

function headingLevel(tag: string): HeadingLevel | undefined {
  switch (tag) {
    case 'h1':
      return HeadingLevel.HEADING_1
    case 'h2':
      return HeadingLevel.HEADING_2
    case 'h3':
      return HeadingLevel.HEADING_3
    case 'h4':
      return HeadingLevel.HEADING_4
    case 'h5':
      return HeadingLevel.HEADING_5
    case 'h6':
      return HeadingLevel.HEADING_6
    default:
      return undefined
  }
}

function inlineToRuns(tokens: any[]): TextRun[] {
  const runs: TextRun[] = []
  let bold = 0
  let italics = 0

  for (const token of tokens) {
    switch (token.type) {
      case 'strong_open':
        bold++
        break
      case 'strong_close':
        bold = Math.max(0, bold - 1)
        break
      case 'em_open':
        italics++
        break
      case 'em_close':
        italics = Math.max(0, italics - 1)
        break
      case 'code_inline':
        runs.push(
          new TextRun({
            text: token.content ?? '',
            font: 'Consolas',
            size: 20,
            bold: bold > 0,
            italics: italics > 0
          })
        )
        break
      case 'text':
        runs.push(
          new TextRun({
            text: token.content ?? '',
            bold: bold > 0,
            italics: italics > 0
          })
        )
        break
      case 'link_open': {
        // Skip URL styling; keep text content and append URL in brackets if available.
        const href = token.attrs?.find((a: any) => a?.[0] === 'href')?.[1]
        if (href) {
          runs.push(
            new TextRun({
              text: `(${href})`,
              bold: bold > 0,
              italics: italics > 0
            })
          )
        }
        break
      }
      default:
        break
    }
  }

  return runs.length > 0 ? runs : [new TextRun({ text: '' })]
}

export async function markdownToDocxBytes(markdown: string): Promise<Uint8Array> {
  const tokens = md.parse(markdown, {})

  const elements: Paragraph[] = []
  let listLevel = 0
  let inListItem = false

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i]

    switch (token.type) {
      case 'bullet_list_open':
      case 'ordered_list_open':
        listLevel++
        break
      case 'bullet_list_close':
      case 'ordered_list_close':
        listLevel = Math.max(0, listLevel - 1)
        break
      case 'list_item_open':
        inListItem = true
        break
      case 'list_item_close':
        inListItem = false
        break

      case 'heading_open': {
        const level = headingLevel(token.tag)
        const inline = tokens[i + 1]
        const runs = inline?.type === 'inline' ? inlineToRuns(inline.children || []) : [new TextRun({ text: '' })]
        elements.push(new Paragraph({ heading: level, children: runs }))
        break
      }

      case 'paragraph_open': {
        const inline = tokens[i + 1]
        if (inline?.type !== 'inline') break
        const runs = inlineToRuns(inline.children || [])
        const bullet = inListItem ? { level: Math.max(0, listLevel - 1) } : undefined
        elements.push(new Paragraph({ children: runs, bullet }))
        break
      }

      case 'fence':
      case 'code_block': {
        const code = String(token.content ?? '')
        const lines = code.split(/\r?\n/)
        for (const line of lines) {
          elements.push(
            new Paragraph({
              children: [new TextRun({ text: line, font: 'Consolas', size: 20 })]
            })
          )
        }
        break
      }

      default:
        break
    }
  }

  const doc = new Document({
    sections: [
      {
        properties: {},
        children: elements
      }
    ]
  })

  const blob = await Packer.toBlob(doc)
  const buffer = await blob.arrayBuffer()
  return new Uint8Array(buffer)
}

