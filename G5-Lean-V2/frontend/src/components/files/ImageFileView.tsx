// =============================================================================
// G5 Files — ImageFileView
// =============================================================================
// A raster image or SVG shown through the browser's own decoder in an
// `<img>` over a typed object URL (`g5.documents.viewers.images`): Fit to
// panel, Actual size, Zoom in and out, scrolling when enlarged, a
// checkerboard behind transparency, and Download. Before an image decodes,
// its header is probed so a declared size above the pixel bound is refused
// without decoding; a decode failure keeps Download. Transparency, animation
// and EXIF orientation are the browser's behaviour for `<img>`, and an SVG
// in an image context runs no script.
// =============================================================================

import React, { useEffect, useMemo, useRef, useState } from "react";
import { Download, ZoomIn, ZoomOut } from "lucide-react";
import { classNames } from "../ui/UI";
import { downloadBlob } from "../../lib/utils";
import { FileNoticeCard } from "./FileNoticeCard";
import { formatByteSize, useObjectUrl } from "./fileBytes";
import { imageDimensions, type ImageDimensions } from "./imageDimensions";
import {
  imageScale,
  MAX_IMAGE_PIXELS,
  MAX_ZOOM,
  MIN_ZOOM,
  steppedZoom,
  type ImageZoom,
  type PanelSize,
} from "./imageZoom";
import "./fileViewers.css";

const STAGE_PADDING = 16;

const toolbarButton =
  "flex shrink-0 items-center gap-1 rounded px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:cursor-not-allowed disabled:opacity-50";

export interface ImageFileViewProps {
  filePath: string;
  /** The image's bytes, typed by its extension. */
  blob: Blob;
  /** The bytes behind `blob`, for the header probe; absent for SVG text. */
  bytes?: Uint8Array | null;
  byteSize: number;
  /**
   * Under the editor's own file row (an SVG's rendered mode) the viewer
   * shows only its controls; standalone it names the file too.
   */
  embedded?: boolean;
}

export const ImageFileView: React.FC<ImageFileViewProps> = ({
  filePath,
  blob,
  bytes = null,
  byteSize,
  embedded = false,
}) => {
  const name = filePath.split("/").pop() || filePath;
  const url = useObjectUrl(blob);
  const declared = useMemo(
    () => (bytes ? imageDimensions(bytes) : null),
    [bytes],
  );
  const tooLarge =
    declared !== null && declared.width * declared.height > MAX_IMAGE_PIXELS;

  // What the browser reported for the URL now shown; a new URL starts over.
  const [outcome, setOutcome] = useState<
    | { url: string; status: "loaded"; natural: ImageDimensions }
    | { url: string; status: "failed" }
    | null
  >(null);
  const natural =
    outcome?.url === url && outcome.status === "loaded"
      ? outcome.natural
      : null;
  const failed = outcome?.url === url && outcome.status === "failed";

  const [zoom, setZoom] = useState<ImageZoom>({ mode: "fit" });
  const stageRef = useRef<HTMLDivElement>(null);
  const [panel, setPanel] = useState<PanelSize | null>(null);
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const measure = () =>
      setPanel({
        width: stage.clientWidth - 2 * STAGE_PADDING,
        height: stage.clientHeight - 2 * STAGE_PADDING,
      });
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, []);

  const scale = imageScale(zoom, natural, panel);
  const shownSize = natural
    ? {
        width: Math.max(1, Math.round(natural.width * scale)),
        height: Math.max(1, Math.round(natural.height * scale)),
      }
    : null;
  const download = () => downloadBlob(blob, name);
  const describe = natural ?? declared;

  return (
    <div
      data-testid="image-view"
      data-file-path={filePath}
      className="flex h-full min-h-0 flex-col bg-[var(--g5-bg1)]"
    >
      <div
        data-testid="image-view-file-row"
        className="flex shrink-0 flex-wrap items-center gap-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs text-[var(--g5-text2)]"
      >
        {!embedded && (
          <span className="min-w-0 truncate font-medium text-[var(--g5-text0)]">
            {name}
          </span>
        )}
        <span data-testid="image-view-dimensions" className="shrink-0">
          {describe ? `${describe.width} × ${describe.height} · ` : ""}
          {formatByteSize(byteSize)}
        </span>
        <div className="flex-1" />
        <button
          type="button"
          data-testid="image-view-fit"
          className={classNames(
            toolbarButton,
            zoom.mode === "fit" && "text-[var(--g5-active)]",
          )}
          disabled={!natural}
          onClick={() => setZoom({ mode: "fit" })}
        >
          Fit
        </button>
        <button
          type="button"
          data-testid="image-view-actual"
          className={classNames(
            toolbarButton,
            zoom.mode === "actual" && "text-[var(--g5-active)]",
          )}
          disabled={!natural}
          onClick={() => setZoom({ mode: "actual" })}
        >
          Actual size
        </button>
        <button
          type="button"
          data-testid="image-view-zoom-out"
          aria-label="Zoom out"
          title="Zoom out"
          className={toolbarButton}
          disabled={!natural || scale <= MIN_ZOOM}
          onClick={() =>
            setZoom({ mode: "scale", scale: steppedZoom(scale, -1) })
          }
        >
          <ZoomOut className="h-3.5 w-3.5" />
        </button>
        <span
          data-testid="image-view-zoom-level"
          className="w-10 shrink-0 text-center tabular-nums"
        >
          {natural ? `${Math.round(scale * 100)}%` : ""}
        </span>
        <button
          type="button"
          data-testid="image-view-zoom-in"
          aria-label="Zoom in"
          title="Zoom in"
          className={toolbarButton}
          disabled={!natural || scale >= MAX_ZOOM}
          onClick={() =>
            setZoom({ mode: "scale", scale: steppedZoom(scale, 1) })
          }
        >
          <ZoomIn className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          data-testid="image-view-download"
          className={toolbarButton}
          onClick={download}
        >
          <Download className="h-3.5 w-3.5" />
          Download
        </button>
      </div>
      <div
        ref={stageRef}
        data-testid="image-view-stage"
        className="g5-image-stage min-h-0 flex-1 overflow-auto"
      >
        {tooLarge ? (
          <FileNoticeCard
            testId="image-view-notice"
            reason="too-large"
            filePath={filePath}
            detail={`Too large to display: ${declared.width} × ${declared.height} pixels is more than the ${Math.round(MAX_IMAGE_PIXELS / 1_000_000)} megapixels this viewer decodes.`}
            onDownload={download}
          />
        ) : failed ? (
          <FileNoticeCard
            testId="image-view-notice"
            reason="undecodable"
            filePath={filePath}
            detail="This image can't be shown. The file may be corrupt, misnamed or in a format this browser does not decode."
            onDownload={download}
          />
        ) : (
          <div className="g5-image-stage-inner">
            {url && (
              <img
                data-testid="image-view-image"
                data-status={natural ? "loaded" : "loading"}
                src={url}
                alt={name}
                draggable={false}
                className="g5-checkerboard"
                style={shownSize ?? undefined}
                onLoad={(event) =>
                  setOutcome({
                    url,
                    status: "loaded",
                    natural: {
                      width: event.currentTarget.naturalWidth,
                      height: event.currentTarget.naturalHeight,
                    },
                  })
                }
                onError={() => setOutcome({ url, status: "failed" })}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
};
