// Copying from a session's conversation: a button that copies some text and says so for a
// moment, and a code block that shows one when you hover it.

import { useEffect, useState } from 'react';
import { Icon } from './primitives';

export function CopyButton({ text, title = 'Copy', className }: { text: string; title?: string; className?: string }) {
  const [copied, setCopied] = useState<'yes' | 'no' | null>(null);
  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(null), 1500);
    return () => window.clearTimeout(id);
  }, [copied]);
  return (
    <button
      type="button"
      className={['ca-copybtn', copied === 'yes' && 'is-copied', className].filter(Boolean).join(' ')}
      title={copied === 'yes' ? 'Copied' : copied === 'no' ? 'Couldn’t copy' : title}
      aria-label={title}
      onClick={(e) => {
        e.stopPropagation();
        navigator.clipboard.writeText(text).then(
          () => setCopied('yes'),
          () => setCopied('no'),
        );
      }}
    >
      <Icon name={copied === 'yes' ? 'check' : 'copy'} size={13} stroke={copied === 'yes' ? 2.25 : 1.75} />
      {copied && <span>{copied === 'yes' ? 'Copied' : 'Couldn’t copy'}</span>}
    </button>
  );
}

/** Preformatted text you can select, with a Copy button on hover. */
export function CodeBlock({ text, className }: { text: string; className?: string }) {
  return (
    <div className="ca-codeblock">
      <pre className={className}>{text}</pre>
      <CopyButton text={text} title="Copy" />
    </div>
  );
}
