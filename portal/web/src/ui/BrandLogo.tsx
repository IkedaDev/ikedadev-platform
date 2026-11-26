import { useEffect, useState } from 'react';

export interface BrandLogoProps {
  logoUrl?: string | null;
  name?: string;
  className?: string;
}

function getInitials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return 'KR';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return `${words[0][0]}${words[1][0]}`.toUpperCase();
}

export function BrandLogo({ logoUrl, name = 'Kropy', className = 'brand-mark' }: BrandLogoProps) {
  const [imgError, setImgError] = useState(false);

  useEffect(() => {
    setImgError(false);
  }, [logoUrl]);

  const trimmedUrl = logoUrl?.trim();
  const displayName = name.trim() || 'Kropy';

  return (
    <div className={className}>
      {trimmedUrl && !imgError ? (
        <img
          src={trimmedUrl}
          alt={displayName}
          className="brand-logo-img"
          onError={() => setImgError(true)}
        />
      ) : (
        getInitials(displayName)
      )}
    </div>
  );
}
