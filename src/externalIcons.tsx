import type { SVGProps } from "react";

const stroke = { fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

export function IconGithub(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <path
        {...stroke}
        d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"
      />
    </svg>
  );
}

export function IconTwitch(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <path
        fill="currentColor"
        d="M11.6 4.7h1.7v5.2h-1.7zm4.7 0H18v5.2h-1.7zM6 0 1.7 4.3v15.4h5.2V24l4.3-4.3h3.4L22.3 12V0H6zm14.6 11.1-3.4 3.4h-3.5l-3 3v-3H6.9V1.7h13.7v9.4Z"
      />
    </svg>
  );
}

export function IconGlobe(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <circle {...stroke} cx="12" cy="12" r="10" />
      <path {...stroke} d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
    </svg>
  );
}

/** Row action: delete stream */
export function IconTrash(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <path {...stroke} d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m2 0v14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V6h12M10 11v6M14 11v6" />
    </svg>
  );
}

/** Row action: start stream */
export function IconPlay(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <path {...stroke} d="M8 5v14l11-7-11-7z" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** Row action: stop stream */
export function IconStopSquare(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden {...props}>
      <rect x="6" y="6" width="12" height="12" rx="1" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** Shown while start/stop is in progress for a row */
export function IconSpinner(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={18} height={18} aria-hidden className="icon-spinner" {...props}>
      <circle {...stroke} cx="12" cy="12" r="9" strokeDasharray="42" strokeDashoffset="12" opacity={0.35} />
      <circle {...stroke} cx="12" cy="12" r="9" strokeDasharray="14 28" />
    </svg>
  );
}
