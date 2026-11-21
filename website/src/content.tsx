import type { ReactNode } from 'react';

export const VERSION = '0.1.3';

export const REPO = 'https://github.com/greenbugx/ANSImuz';

export const LINKS = {
  repo: REPO,
  releases: `${REPO}/releases/latest`,
  issues: `${REPO}/issues`,
  readme: `${REPO}#readme`,
  contributing: `${REPO}/blob/main/CONTRIBUTING.md`,
  license: `${REPO}/blob/main/LICENSE`,
  thirdParty: `${REPO}/blob/main/THIRD-PARTY-LICENSES.md`,
  conduct: `${REPO}/blob/main/CODE_OF_CONDUCT.md`,
  copr: 'https://copr.fedorainfracloud.org/coprs/greenbugx/ansimuz/',
  sourceforge: 'https://sourceforge.net/projects/ansimuz/files/latest/download',
  mpv: 'https://mpv.io/',
  ytdlp: 'https://github.com/yt-dlp/yt-dlp',
};

export const SECTIONS = [
  { id: 'features', index: '01', label: 'Features' },
  { id: 'docs', index: '02', label: 'Docs' },
  { id: 'download', index: '03', label: 'Download' },
  { id: 'faq', index: '04', label: 'FAQ' },
] as const;

export const SECTION_IDS = SECTIONS.map((section) => section.id);

export type Feature = {
  title: string;
  description: ReactNode;
};

export const FEATURES: Feature[] = [
  {
    title: 'Multi-source streaming',
    description: 'Search YouTube Music and Spotify, then play both from the same queue.',
  },
  {
    title: 'Synchronized lyrics',
    description:
      'Words and syllables highlight in time with the track. Duets and background vocals get their own lanes.',
  },
  {
    title: 'ANSI album art',
    description: 'Covers are drawn in adaptive block art or get Ultra High Quality cover arts right inside the terminal.',
  },
  {
    title: 'Queue control',
    description: 'Add, remove, and reorder tracks with a few keys.',
  },
  {
    title: 'Session history',
    description: 'Jump back to anything you played earlier in the current session.',
  },
  {
    title: 'Adaptive layout',
    description: 'The interface reshapes itself to fit the terminal you have open.',
  },
  {
    title: 'Color themes',
    description: 'Palettes sampled from the album art, or classic ANSI colors if you prefer.',
  },
  {
    title: 'Command palette',
    description: (
      <>
        Press <kbd>:</kbd> to reach every command and setting from one place.
      </>
    ),
  },
  {
    title: 'Offline cache',
    description: 'Audio, artwork, and lyrics stay cached for instant replay, even offline.',
  },
];

export type Control = {
  key: string;
  action: string;
};

export const CONTROLS: Control[] = [
  { key: 'space', action: 'Play or pause the current track' },
  { key: '← / →', action: 'Previous track or next in queue' },
  { key: 'L', action: 'Open or close the lyrics view' },
  { key: ':', action: 'Open the command palette' },
  { key: 'S', action: 'Open settings' },
  { key: 'X', action: 'Toggle queue shuffle' },
  { key: 'R', action: 'Cycle repeat mode' },
  { key: '?', action: 'Show the about overlay' },
  { key: 'Q', action: 'Quit ANSImuz' },
];

export type Platform = {
  name: string;
  meta: string;
  commandLabel?: string;
  commands?: string[];
  note?: ReactNode;
  action: string;
  href: string;
  primary?: boolean;
  secondary?: {
    label: string;
    href: string;
  };
};

export const PLATFORMS: Platform[] = [
  {
    name: 'Fedora',
    meta: 'COPR repository',
    commandLabel: 'install',
    commands: ['sudo dnf copr enable greenbugx/ansimuz', 'sudo dnf install ansimuz'],
    note: (
      <>
        Or download the prebuilt binary from{' '}
        <a href={LINKS.releases} target="_blank" rel="noreferrer">
          GitHub Releases
        </a>
        .
      </>
    ),
    action: 'Open COPR',
    href: LINKS.copr,
  },
  {
    name: 'Arch Linux',
    meta: 'PKGBUILD',
    commandLabel: 'build',
    commands: [
      'git clone https://github.com/greenbugx/ANSImuz.git',
      'cd ANSImuz/packaging/arch',
      'makepkg -si',
    ],
    note: (
      <>
        Or download the prebuilt binary from{' '}
        <a href={LINKS.releases} target="_blank" rel="noreferrer">
          GitHub Releases
        </a>
        .
      </>
    ),
    action: 'View packaging',
    href: `${REPO}/tree/main/packaging/arch`,
  },
  {
    name: 'Windows',
    meta: 'Portable x64 build',
    commandLabel: 'prerequisites',
    commands: [
      'winget install --id shinchiro.mpv -e',
      'winget install --id yt-dlp.yt-dlp -e',
    ],
    note: 'The release bundles its runtime. Extract the archive and run ansimuz.exe. Windows 10 and 11 are supported.',
    action: 'Download .rar',
    href: LINKS.releases,
    primary: true,
    secondary: {
      label: 'SourceForge mirror',
      href: LINKS.sourceforge,
    },
  },
  {
    name: 'From source',
    meta: 'Rust toolchain',
    note: 'The full build requirements and commands live in CONTRIBUTING.md.',
    action: 'Build guide',
    href: LINKS.contributing,
  },
];

export type Faq = {
  question: string;
  answer: ReactNode;
};

export const FAQS: Faq[] = [
  {
    question: 'Do I need a Spotify or YouTube account?',
    answer:
      'No. ANSImuz searches public catalog data and plays the result. You do not need to log in to either service.',
  },
  {
    question: 'Do I need Python installed?',
    answer:
      'Not for the Windows portable build. It bundles its runtime. The Linux packages handle dependencies for you. Building from source needs the toolchain described in CONTRIBUTING.md.',
  },
  {
    question: 'Why does it need mpv and yt-dlp?',
    answer:
      'mpv handles audio playback and yt-dlp resolves the audio stream. Both need to be installed and available on your PATH before you launch ANSImuz.',
  },
  {
    question: 'Is Discord Rich Presence stable?',
    answer:
      'It is experimental. It works, but expect rough edges while it is being developed. You can turn it on or off from Settings.',
  },
  {
    question: 'Where does it store caches?',
    answer: (
      <>
        Under <kbd>$XDG_CACHE_HOME/ansimuz</kbd> on Linux or{' '}
        <kbd>%LOCALAPPDATA%\ansimuz</kbd> on Windows. Audio, artwork, and lyrics
        are kept there. You can clear them from Settings.
      </>
    ),
  },
  {
    question: 'Does it run on Windows?',
    answer:
      'Yes. Use the portable release for Windows 10 and 11, or run it through WSL2 if you prefer a Linux environment.',
  },
  {
    question: 'Is it open source?',
    answer:
      'Yes. ANSImuz is released under the MIT license. The source, issues, and release notes are all on GitHub.',
  },
  {
    question: 'Will album art work in my terminal?',
    answer:
      'The interface runs in any terminal with ANSI color support. Image quality depends on the terminal image protocol, but the player and lyrics stay usable either way.',
  },
];

export const ASCII_LOGO = `   ▄████████ ███▄▄▄▄      ▄████████  ▄█    ▄▄▄▄███▄▄▄▄   ███    █▄   ▄███████▄  
  ███    ███ ███▀▀▀██▄   ███    ███ ███  ▄██▀▀▀███▀▀▀██▄ ███    ███ ██▀     ▄██ 
  ███    ███ ███   ███   ███    █▀  ███▌ ███   ███   ███ ███    ███       ▄███▀ 
  ███    ███ ███   ███   ███        ███▌ ███   ███   ███ ███    ███  ▀█▀▄███▀▄▄ 
▀███████████ ███   ███ ▀███████████ ███▌ ███   ███   ███ ███    ███   ▄███▀   ▀ 
  ███    ███ ███   ███          ███ ███  ███   ███   ███ ███    ███ ▄███▀       
  ███    ███ ███   ███    ▄█    ███ ███  ███   ███   ███ ███    ███ ███▄     ▄█ 
  ███    █▀   ▀█   █▀   ▄████████▀  █▀    ▀█   ███   █▀  ████████▀   ▀████████▀ `;
