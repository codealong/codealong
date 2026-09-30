// Copying from a session's conversation: a button that copies some text and says so for a
// moment, and a code block that shows one when you hover it, with Run beside it for shell
// commands (RunButton.tsx).

import { useContext, useEffect, useState } from 'react';
import { Icon } from './primitives';
import { RunButton, RunContext } from './RunButton';

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

/** Preformatted text you can select, with a Copy button on hover. `run` marks shell commands,
 *  which get a Run button too in a session on this Mac, run where `run.folder` says they ran or
 *  else where the session works. */
export function CodeBlock({ text, className, run }: { text: string; className?: string; run?: { folder?: string } }) {
  const place = useContext(RunContext);
  const folder = run && place ? (run.folder ?? place.folder) : null;
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <div className="ca-codeblock">
      <pre className={className}>{text}</pre>
      <div className="ca-codeblock-tools">
        {folder && place && <RunButton text={text} folder={folder} terminal={place.terminal} onFailed={setFailed} />}
        <CopyButton text={text} title="Copy" />
      </div>
      {failed && (
        <p className="ca-codeblock-note" role="alert">
          {failed}
        </p>
      )}
    </div>
  );
}
