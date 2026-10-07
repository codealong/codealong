// Copying from a session's conversation: a button that copies some text and says so for a
// moment, and a code block that shows one when you hover it, with Run beside it for shell
// commands (RunButton.tsx).

import { useContext, useEffect, useState, type ReactNode } from 'react';
import { cx, Icon } from './primitives';
import { RunButton, RunContext, sessionCommand, terminalCommand, type RunTarget } from './RunButton';

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
      className={cx('ca-copybtn', copied === 'yes' && 'is-copied', className)}
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
 *  which get a Run button too in a session on this Mac, run in a new terminal where `run.folder`
 *  says they ran or else where the session works. `run.session` marks a command for the
 *  session's own shell (led by `!`), which goes into the session instead where it can, and
 *  otherwise into the terminal without its `!`. `children` is the text as it's drawn, when
 *  that's more than the text: a tool's output with its links. */
export function CodeBlock({ text, className, run, children }: { text: string; className?: string; run?: { folder?: string; session?: boolean }; children?: ReactNode }) {
  const place = useContext(RunContext);
  const target: RunTarget | null = !run || !place ? null : run.session && place.session ? { session: place.session } : { folder: run.folder ?? place.folder, terminal: place.terminal };
  const command = !run?.session ? text : target && 'session' in target ? sessionCommand(text) : terminalCommand(text);
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <div className="ca-codeblock">
      <pre className={className}>{children ?? text}</pre>
      <div className="ca-codeblock-tools">
        {target && <RunButton text={command} target={target} onFailed={setFailed} />}
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
