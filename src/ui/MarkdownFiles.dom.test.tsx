// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { act, engine, settle } from '../testing/dom';
import { createRoot } from 'react-dom/client';
import { ImagesFrom } from './DescriptionEditor';
import { Markdown } from './Markdown';
import { SentFiles } from './SentFiles';

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}

function mount(node: React.ReactNode) {
  const el = document.body.appendChild(document.createElement('div'));
  const root = createRoot(el);
  act(() => root.render(node));
  return {
    el,
    unmount: () => {
      act(() => root.unmount());
      el.remove();
    },
  };
}

describe('local file links, images and Markdown previews', () => {
  it('opens a file:// Markdown link in the in-app preview dialog with carousel navigation, and opens #L anchors in the editor', async () => {
    const MD = '/Users/me/.gemini/antigravity-cli/brain/id/mocks.md';
    const RS = '/Users/me/projects/app/src/main.rs';
    const IMG_A = '/Users/me/.gemini/antigravity-cli/brain/id/light.png';
    const IMG_B = '/Users/me/.gemini/antigravity-cli/brain/id/dark.png';

    engine.answer('path_kinds', (args: Record<string, unknown>) => {
      const paths = args.paths as string[];
      return paths.map((p) => (p === MD || p === RS ? 'file' : null));
    });
    engine.answer('sent_files', (args: Record<string, unknown>) => (args.paths as string[]).map(() => 'here'));
    engine.answer('your_editor', () => ({ id: 'zed', name: 'Zed' }));
    engine.answer('read_markdown', (args: Record<string, unknown>) => {
      expect(args.path).toBe(MD);
      return {
        kind: 'text',
        text: [
          '# Mocks',
          '',
          '````carousel',
          `![Light mode](${IMG_A})`,
          '<!-- slide -->',
          `![Dark mode](${IMG_B})`,
          '````',
        ].join('\n'),
      };
    });

    const view = mount(
      <Markdown text={`See [mocks.md](file://${MD}), [main.rs](file://${RS}#L42), and [missing.md](file:///Users/me/gone.md).`} />,
    );
    await settle();
    await settle();

    // Existing files become links; missing files stay plain text.
    const links = [...view.el.querySelectorAll('a.ca-md-a')];
    expect(links.map((a) => a.textContent)).toEqual(['mocks.md', 'main.rs']);

    // Clicking a line-anchored file:// link opens it in the editor at that line.
    act(() => links[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })));
    await settle();
    expect(engine.sent('edit_file')).toEqual([{ path: RS, line: 42 }]);

    // Clicking a file:// Markdown link opens the in-app preview dialog and renders the carousel.
    act(() => links[0].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })));
    await settle();
    await settle();
    const dlg = document.body.querySelector('.ca-dialog.ca-mdpreview');
    expect(dlg).not.toBeNull();
    expect(dlg?.querySelector('h2')?.textContent).toBe('mocks.md');
    expect(dlg?.querySelector('.ca-md-carousel-head')?.textContent).toContain('Slide 1 of 2');
    expect(dlg?.querySelector('.ca-md-carousel img')?.getAttribute('alt')).toBe('Light mode');

    // Advancing the carousel switches to slide 2.
    const next = dlg?.querySelector('button[aria-label="Next slide"]') as HTMLButtonElement;
    act(() => next.click());
    await settle();
    await settle();
    expect(dlg?.querySelector('.ca-md-carousel-head')?.textContent).toContain('Slide 2 of 2');
    expect(dlg?.querySelector('.ca-md-carousel img')?.getAttribute('alt')).toBe('Dark mode');

    view.unmount();
  });

  it('gates macOS-protected local images behind a Show image button, and hides local images on remote sessions', async () => {
    const PROTECTED = '/Users/me/Desktop/shot.png';
    engine.answer('sent_files', (args: Record<string, unknown>) => (args.paths as string[]).map((p) => (p === PROTECTED ? 'asksFirst' : 'here')));

    const local = mount(<Markdown text={`![Desktop shot](${PROTECTED})`} />);
    await settle();
    await settle();
    expect(local.el.querySelector('img')).toBeNull();
    const show = local.el.querySelector('.ca-md-img-ask button') as HTMLButtonElement;
    expect(show.textContent).toContain('Show image');

    act(() => show.click());
    await settle();
    expect(local.el.querySelector('img')?.getAttribute('alt')).toBe('Desktop shot');
    local.unmount();

    const remote = mount(
      <ImagesFrom.Provider value="mac2">
        <Markdown text={`![Desktop shot](${PROTECTED})`} />
      </ImagesFrom.Provider>,
    );
    await settle();
    expect(remote.el.querySelector('img')).toBeNull();
    expect(remote.el.textContent).toContain('on the session’s machine');
    remote.unmount();
  });

  it('previews a Markdown artifact listed in SentFiles and handles asksFirst before reading', async () => {
    const DOC = '/Users/me/Documents/plan.md';
    engine.answer('sent_files', () => []);
    engine.answer('your_editor', () => ({ id: 'cursor', name: 'Cursor' }));
    engine.answer('read_markdown', (args: Record<string, unknown>) => {
      if (!args.asked) return { kind: 'asksFirst' };
      return { kind: 'text', text: '# Plan\n\nReady to build.' };
    });

    const view = mount(<SentFiles sent={{ files: [DOC], caption: 'Review the plan' }} at={7} />);
    await settle();
    const btn = view.el.querySelector('button.ca-md-file') as HTMLButtonElement;
    expect(btn.title).toBe(`Preview ${DOC}`);

    act(() => btn.click());
    await settle();
    await settle();
    const dlg = document.body.querySelector('.ca-dialog.ca-mdpreview')!;
    expect(dlg.querySelector('.ca-mdpreview-ask')).not.toBeNull();

    const allow = dlg.querySelector('.ca-mdpreview-ask button') as HTMLButtonElement;
    act(() => allow.click());
    await settle();
    await settle();
    expect(dlg.textContent).toContain('Ready to build.');

    view.unmount();
  });
});
