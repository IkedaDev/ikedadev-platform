import Image from '@tiptap/extension-image';
import Link from '@tiptap/extension-link';
import Placeholder from '@tiptap/extension-placeholder';
import StarterKit from '@tiptap/starter-kit';
import { EditorContent, useEditor } from '@tiptap/react';
import {
  Bold,
  Code2,
  Heading2,
  Heading3,
  Image as ImageIcon,
  Italic,
  Link2,
  List,
  ListOrdered,
  Quote,
} from 'lucide-react';
import { useState } from 'react';
import { Button, Input } from '../../ui';
import { isSafeMediaUrl, useMediaPicker } from '../shared/MediaPickerModal';

export interface RichTextEditorProps {
  initialContentJson?: Record<string, unknown>;
  initialContentHtml?: string;
  disabled?: boolean;
  onChange: (payload: { json: Record<string, unknown>; html: string }) => void;
}

export function RichTextEditor({
  initialContentJson,
  initialContentHtml,
  disabled = false,
  onChange,
}: RichTextEditorProps) {
  const { openMediaPicker } = useMediaPicker();
  const [linkPanelOpen, setLinkPanelOpen] = useState(false);
  const [linkUrl, setLinkUrl] = useState('');

  const hasInitialJson =
    initialContentJson &&
    typeof initialContentJson === 'object' &&
    Object.keys(initialContentJson).length > 0 &&
    initialContentJson.type === 'doc';

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [2, 3] },
      }),
      Image.configure({
        HTMLAttributes: { class: 'editorial-img' },
      }),
      Link.configure({
        openOnClick: false,
        HTMLAttributes: {
          rel: 'noopener noreferrer',
          target: '_blank',
        },
      }),
      Placeholder.configure({
        placeholder: 'Escribe tu artículo aquí… Usa la barra superior para encabezados, citas, código o imágenes.',
      }),
    ],
    content: hasInitialJson ? initialContentJson : initialContentHtml || '',
    editable: !disabled,
    onUpdate: ({ editor: ed }) => {
      onChange({
        json: ed.getJSON() as Record<string, unknown>,
        html: ed.getHTML(),
      });
    },
  });

  if (!editor) {
    return null;
  }

  const handleOpenImagePicker = () => {
    setLinkPanelOpen(false);
    openMediaPicker({
      context: 'content',
      title: 'Insertar imagen en el artículo',
      onSelect: (url, asset) => {
        editor
          .chain()
          .focus()
          .setImage({ src: url, alt: asset?.alt_text || asset?.file_name || '' })
          .run();
      },
    });
  };

  const handleApplyLink = () => {
    const trimmed = linkUrl.trim();
    if (!trimmed) {
      editor.chain().focus().unsetLink().run();
      setLinkPanelOpen(false);
      return;
    }
    if (!isSafeMediaUrl(trimmed)) return;
    editor.chain().focus().extendMarkRange('link').setLink({ href: trimmed }).run();
    setLinkUrl('');
    setLinkPanelOpen(false);
  };

  return (
    <div className="tiptap-shell">
      {!disabled && (
        <div className="tiptap-toolbar" role="toolbar" aria-label="Formato de texto">
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('heading', { level: 2 }) ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
            title="Título H2"
          >
            <Heading2 size={15} />
          </button>
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('heading', { level: 3 }) ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
            title="Subtítulo H3"
          >
            <Heading3 size={15} />
          </button>
          <span className="tiptap-divider" />
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('bold') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleBold().run()}
            title="Negrita"
          >
            <Bold size={15} />
          </button>
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('italic') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleItalic().run()}
            title="Cursiva"
          >
            <Italic size={15} />
          </button>
          <span className="tiptap-divider" />
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('bulletList') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleBulletList().run()}
            title="Lista con viñetas"
          >
            <List size={15} />
          </button>
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('orderedList') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleOrderedList().run()}
            title="Lista numerada"
          >
            <ListOrdered size={15} />
          </button>
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('blockquote') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleBlockquote().run()}
            title="Cita"
          >
            <Quote size={15} />
          </button>
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('codeBlock') ? 'is-active' : ''}`}
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}
            title="Bloque de código"
          >
            <Code2 size={15} />
          </button>
          <span className="tiptap-divider" />
          <button
            type="button"
            className={`tiptap-tool ${editor.isActive('link') || linkPanelOpen ? 'is-active' : ''}`}
            onClick={() => {
              setLinkUrl((editor.getAttributes('link').href as string) || '');
              setLinkPanelOpen((v) => !v);
            }}
            title="Insertar enlace"
          >
            <Link2 size={15} />
          </button>
          <button
            type="button"
            className="tiptap-tool"
            onClick={handleOpenImagePicker}
            title="Insertar imagen (Biblioteca de Medios o URL externa)"
          >
            <ImageIcon size={15} />
          </button>
        </div>
      )}

      {linkPanelOpen && !disabled && (
        <div className="tiptap-popover">
          <div className="row-sm">
            <Input
              placeholder="https://ejemplo.com/recurso"
              value={linkUrl}
              onChange={(e) => setLinkUrl(e.target.value)}
            />
            <Button size="sm" variant="primary" onClick={handleApplyLink}>
              Aplicar
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setLinkPanelOpen(false)}>
              Cancelar
            </Button>
          </div>
        </div>
      )}

      <EditorContent editor={editor} className="tiptap-content" />
    </div>
  );
}
