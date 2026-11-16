/** The OnSet mark: a transient (flat line, one sharp spike, decaying ripples) in a rounded
 *  square. Same geometry as OnSet's assets/brand/onset-mark.svg and app icon. */
export function Logo({size=28,title='OnSet'}:{size?:number;title?:string}){
  return <svg width={size} height={size} viewBox="0 0 64 64" role="img" aria-label={title}>
    <rect width="64" height="64" rx="15" fill="#0c0d10" stroke="#33363f" strokeWidth="1.5"/>
    <path d="M10 36 H24 L28.5 13 L33 48 L36.5 29 L40 40 L43.5 34 L47 37.5 L50 36 H54" fill="none"
      stroke="#a194ff" strokeWidth="3.6" strokeLinecap="round" strokeLinejoin="round"/>
    <circle cx="28.5" cy="13" r="3.4" fill="#f472b6"/>
  </svg>;
}
