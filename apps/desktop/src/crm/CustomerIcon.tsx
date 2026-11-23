const paths = {
  users: 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2m20 0v-2a4 4 0 0 0-3-3.9M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Zm5-4a4 4 0 0 1 0 8',
  grid: 'M3 4h18v16H3zM3 9h18M9 9v11',
  board: 'M4 4h6v16H4zM14 4h6v11h-6z',
  star: 'm12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z',
  box: 'm12 3 9 5v8l-9 5-9-5V8zm0 9v9m-9-13 9 4 9-4',
  growth: 'm3 17 6-6 4 4 7-8m-5 0h5v5',
  checkCircle: 'M22 12a10 10 0 1 1-5-8.7M7.5 11.5l3 3 8-8',
  search: 'M16.5 10a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Zm-2 4.5 5 5',
  filter: 'M3 5h18l-7 8v5l-4 2v-7z',
  settings: 'M4 6h16M4 12h16M4 18h16M9 4v4m6 2v4M9 16v4',
  plus: 'M12 5v14M5 12h14',
  check: 'm5 12 4 4L19 6',
  edit: 'm14 6 4 4M4 20l4.5-1L20 7.5a2.8 2.8 0 0 0-4-4L4.8 15z',
  arrow: 'm9 6 6 6-6 6',
  mail: 'M3 5h18v14H3zm0 1 9 7 9-7',
  contact: 'M4 5h16v14H4zm4 10a3 3 0 0 1 6 0m-1-5a2 2 0 1 1-4 0 2 2 0 0 1 4 0zm3 0h2m-2 3h2',
  note: 'M5 3h14v18H5zm3 5h8m-8 4h8m-8 4h5',
  clock: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9-5v5l3 2',
} as const

export function CustomerIcon({ name }: { name: keyof typeof paths }) {
  return <svg className="customer-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} /></svg>
}
