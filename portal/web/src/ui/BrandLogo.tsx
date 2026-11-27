import { useEffect, useState } from 'react';
import { sanitizeLogoUrl } from '../utils/url';

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

  const safeUrl = sanitizeLogoUrl(logoUrl);

  useEffect(() => {
    setImgError(false);
  }, [safeUrl]);

  const displayName = name.trim() || 'Kropy';

  return (
    <div className={className}>
      {safeUrl && !imgError ? (
        <img
          src={safeUrl}
          alt={displayName}
          className="brand-logo-img"
          referrerPolicy="no-referrer"
          onError={() => setImgError(true)}
        />
      ) : (
        getInitials(displayName)
      )}
    </div>
  );
}
