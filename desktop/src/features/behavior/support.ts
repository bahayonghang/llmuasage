import type { Copy } from "../../app/i18n";
import { shouldPaintSupportData } from "../../app/secondary";
import type { SupportState } from "../../app/types";

export { shouldPaintSupportData };

export function supportCopy(copy: Copy, level: string | undefined): string {
  switch (level) {
    case "normalized":
      return copy.supportNormalized;
    case "no_data":
      return copy.supportNoData;
    case "degraded":
      return copy.supportDegraded;
    case "unsupported":
      return copy.supportUnsupported;
    case "insufficient_models":
      return copy.supportInsufficient;
    case "low_sample":
      return copy.supportLowSample;
    default:
      return level || copy.supportNoData;
  }
}

export function supportMessage(support: SupportState | undefined, copy: Copy): string {
  return support?.reason || supportCopy(copy, support?.level);
}
