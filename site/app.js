import {
  RELEASES_FALLBACK_URL,
  buildInstallCommands,
  fetchLatestRelease,
  formatPackageOptionLabel,
  getSystemRequirements,
  readReleaseCache,
} from './release-core.mjs';

const HERO_VIEWS = {
  sidebar: {
    light: 'assets/sidebar-light.png',
    dark: 'assets/sidebar-dark.png',
    caption: 'Vertical sidebar and Spaces.',
    alt: 'Rill browser window on Linux with a vertical tab sidebar, pinned site icons, Space switcher dots, and start page',
  },
  toptabs: {
    light: 'assets/top-tabs-light.png',
    dark: 'assets/top-tabs-dark.png',
    caption: 'Horizontal top tabs with Work Space.',
    alt: 'Rill browser window on Linux with horizontal top tabs, Work Space selector, pinned site icons, and start page',
  },
  welcome: {
    light: 'assets/introduction-light.png',
    dark: 'assets/introduction-dark.png',
    caption: 'First-run welcome tab with live theme and layout choices.',
    alt: 'Rill first-run welcome tab with three-ripple mark, Configure button, Use defaults button, and ordered dither wave',
  },
};

const BAYER_8X8 = [
  [0, 32, 8, 40, 2, 34, 10, 42],
  [48, 16, 56, 24, 50, 18, 58, 26],
  [12, 44, 4, 36, 14, 46, 6, 38],
  [60, 28, 52, 20, 62, 30, 54, 22],
  [3, 35, 11, 43, 1, 33, 9, 41],
  [51, 19, 59, 27, 49, 17, 57, 25],
  [15, 47, 7, 39, 13, 45, 5, 37],
  [63, 31, 55, 23, 61, 29, 53, 21],
];

function isDarkScheme() {
  return Boolean(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);
}

function prefersReducedMotion() {
  return Boolean(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
}

function riverGeometry(ny, baseX, phase) {
  const sway = 0.048;
  const cx =
    baseX +
    sway * Math.sin(ny * 2.05 * Math.PI - 0.65 + phase) +
    sway * 0.34 * Math.cos(ny * 4.1 * Math.PI + 0.35 - phase * 0.7);
  const halfW = 0.125 + 0.065 * ny;
  return [cx, halfW];
}

const RIVER_VERT_SRC = `
attribute vec2 a_pos;
void main() {
  gl_Position = vec4(a_pos, 0.0, 1.0);
}
`;

const RIVER_FRAG_SRC = `
precision highp float;
uniform vec2 u_resolution;
uniform float u_dpr;
uniform float u_phase;
uniform float u_dark;
uniform float u_header_bottom;
uniform float u_stage_top;
uniform float u_stage_bottom;
uniform float u_editorial_right;
uniform float u_base_x;

float bayer2(vec2 a) {
  a = floor(a);
  return fract(a.x * 0.5 + a.y * a.y * 0.75);
}

float bayer8(vec2 p) {
  vec2 q = mod(floor(p), 8.0);
  float b = bayer2(mod(q, 2.0))
          + bayer2(floor(mod(q, 4.0) * 0.5)) * 0.25
          + bayer2(floor(q * 0.25)) * 0.0625;
  return b + 0.5 / 64.0;
}

vec2 riverGeom(float ny) {
  float sway = 0.048;
  float cx = u_base_x
    + sway * sin(ny * 2.05 * 3.14159265 - 0.65 + u_phase)
    + sway * 0.34 * cos(ny * 4.1 * 3.14159265 + 0.35 - u_phase * 0.7);
  float halfW = 0.125 + 0.065 * ny;
  return vec2(cx, halfW);
}

void main() {
  vec2 cssPos = vec2(gl_FragCoord.x, u_resolution.y - gl_FragCoord.y) / u_dpr;
  vec2 cssSize = u_resolution / u_dpr;
  float nx = cssPos.x / max(1.0, cssSize.x);
  float ny = cssPos.y / max(1.0, cssSize.y);

  // Pure header bar (0.0 opacity behind nav links), pure hero editorial band, fade out above stage caption bar.
  float belowEditorial = smoothstep(u_stage_top - 8.0, u_stage_top + 28.0, cssPos.y);
  float leftGuard = mix(u_editorial_right, min(u_editorial_right, 0.68), belowEditorial);
  float horizGuard = clamp((nx - leftGuard) / 0.10, 0.0, 1.0);
  float headerFade = smoothstep(u_header_bottom + 6.0, u_header_bottom + 54.0, cssPos.y);
  float bottomFade = 1.0 - smoothstep(u_stage_bottom - 24.0, u_stage_bottom + 8.0, cssPos.y);
  float mask = horizGuard * headerFade * bottomFade;

  vec2 rg = riverGeom(clamp(ny, 0.0, 1.0));
  float cx = rg.x;
  float halfW = max(0.01, rg.y);
  float dist = abs((nx - cx) / halfW);

  vec3 bankDot = mix(vec3(210.0, 206.0, 196.0), vec3(62.0, 56.0, 50.0), u_dark) / 255.0;
  vec3 coreDot = mix(vec3(204.0, 122.0, 92.0), vec3(178.0, 102.0, 72.0), u_dark) / 255.0;
  vec3 accentLine = mix(vec3(183.0, 69.0, 37.0), vec3(246.0, 146.0, 104.0), u_dark) / 255.0;
  vec3 stoneLine = mix(vec3(126.0, 120.0, 110.0), vec3(158.0, 148.0, 136.0), u_dark) / 255.0;

  vec4 outCol = vec4(0.0);

  if (dist < 1.38 && mask > 0.001) {
    float falloff = clamp(1.0 - dist / 1.38, 0.0, 1.0);
    float density = falloff * falloff * mask * 0.42;
    float thresh = bayer8(cssPos / 2.0);
    if (density > thresh) {
      float coreMix = pow(clamp(1.0 - dist / 0.78, 0.0, 1.0), 1.3);
      vec3 dotCol = mix(bankDot, coreDot, coreMix);
      float dotAlpha = (0.58 + 0.34 * coreMix) * headerFade * bottomFade;
      outCol = vec4(dotCol, dotAlpha);
    }
  }

  // Nine anti-aliased topographic river streamlines
  if (mask > 0.001) {
    float lineMask = clamp((nx - (leftGuard + 0.01)) / 0.04, 0.0, 1.0) * headerFade * bottomFade;
    for (int i = 0; i < 9; i++) {
      float offset = 0.0;
      float cPhase = 0.0;
      float isAccent = 0.0;
      float lAlpha = 0.08;
      float strokeW = 1.0;
      if (i == 0) { offset = -1.02; cPhase = 0.20; isAccent = 0.0; lAlpha = 0.08; strokeW = 0.95; }
      else if (i == 1) { offset = -0.72; cPhase = 0.65; isAccent = 0.0; lAlpha = 0.13; strokeW = 1.00; }
      else if (i == 2) { offset = -0.44; cPhase = 1.15; isAccent = 0.0; lAlpha = 0.17; strokeW = 1.05; }
      else if (i == 3) { offset = -0.20; cPhase = 1.55; isAccent = 1.0; lAlpha = 0.22; strokeW = 1.20; }
      else if (i == 4) { offset =  0.00; cPhase = 0.00; isAccent = 1.0; lAlpha = 0.34; strokeW = 1.50; }
      else if (i == 5) { offset =  0.21; cPhase = 2.10; isAccent = 1.0; lAlpha = 0.22; strokeW = 1.20; }
      else if (i == 6) { offset =  0.46; cPhase = 2.55; isAccent = 0.0; lAlpha = 0.17; strokeW = 1.05; }
      else if (i == 7) { offset =  0.75; cPhase = 3.05; isAccent = 0.0; lAlpha = 0.12; strokeW = 1.00; }
      else { offset = 1.06; cPhase = 3.55; isAccent = 0.0; lAlpha = 0.08; strokeW = 0.95; }

      float ripple = 1.0 + 0.12 * sin(ny * 3.0 * 3.14159265 + cPhase + u_phase * 1.4);
      float lineX = (cx + offset * halfW * ripple) * cssSize.x;
      float pxDist = abs(cssPos.x - lineX);
      float cov = 1.0 - smoothstep(strokeW * 0.45, strokeW * 0.45 + 1.05, pxDist);
      float a = cov * lAlpha * lineMask;
      if (a > 0.002) {
        vec3 lc = mix(stoneLine, accentLine, isAccent);
        outCol.rgb = mix(outCol.rgb, lc, clamp(a / max(0.001, outCol.a + a * (1.0 - outCol.a)), 0.0, 1.0));
        outCol.a = outCol.a + a * (1.0 - outCol.a);
      }
    }
  }

  gl_FragColor = outCol;
}
`;

function createWebGLRiver(canvas) {
  const gl = canvas.getContext('webgl', { alpha: true, antialias: false, premultipliedAlpha: false });
  if (!gl) {
    return null;
  }

  function compile(type, src) {
    const sh = gl.createShader(type);
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      gl.deleteShader(sh);
      return null;
    }
    return sh;
  }

  const vs = compile(gl.VERTEX_SHADER, RIVER_VERT_SRC);
  const fs = compile(gl.FRAGMENT_SHADER, RIVER_FRAG_SRC);
  if (!vs || !fs) {
    return null;
  }

  const prog = gl.createProgram();
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    return null;
  }

  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);

  const aPos = gl.getAttribLocation(prog, 'a_pos');
  const uRes = gl.getUniformLocation(prog, 'u_resolution');
  const uDpr = gl.getUniformLocation(prog, 'u_dpr');
  const uPhase = gl.getUniformLocation(prog, 'u_phase');
  const uDark = gl.getUniformLocation(prog, 'u_dark');
  const uHeaderBottom = gl.getUniformLocation(prog, 'u_header_bottom');
  const uStageTop = gl.getUniformLocation(prog, 'u_stage_top');
  const uStageBottom = gl.getUniformLocation(prog, 'u_stage_bottom');
  const uEditorialRight = gl.getUniformLocation(prog, 'u_editorial_right');
  const uBaseX = gl.getUniformLocation(prog, 'u_base_x');

  return {
    clear() {
      if (gl.isContextLost()) {
        return;
      }
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    },
    render(dpr, phase, dark, headerBottom, stageTop, stageBottom, editorialRight, baseX) {
      if (gl.isContextLost()) {
        return;
      }
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(prog);
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
    gl.uniform2f(uRes, canvas.width, canvas.height);
    gl.uniform1f(uDpr, dpr);
    gl.uniform1f(uPhase, phase);
    gl.uniform1f(uDark, dark ? 1 : 0);
    gl.uniform1f(uHeaderBottom, headerBottom);
    gl.uniform1f(uStageTop, stageTop);
    gl.uniform1f(uStageBottom, stageBottom);
    gl.uniform1f(uEditorialRight, editorialRight);
    gl.uniform1f(uBaseX, baseX);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    },
  };
}

function initRiverCanvas() {
  let canvas = document.getElementById('rill-river-canvas');
  if (!canvas) {
    return;
  }

  let glRiver = createWebGLRiver(canvas);
  let ctx = glRiver ? null : canvas.getContext('2d');
  if (!glRiver && !ctx) {
    return;
  }

  let cssWidth = 0;
  let cssHeight = 0;
  let headerBottom = 64;
  let stageTop = 210;
  let stageBottom = 820;
  let editorialRight = 0.72;
  let baseX = 0.875;
  let dpr = 1;
  let offscreen = null;
  let offCtx = null;
  let rafId = 0;
  let startTime = 0;
  let isVisible = true;
  let io = null;

  function fallbackTo2D() {
    if (!glRiver) {
      return;
    }
    glRiver = null;
    const replacement = document.createElement('canvas');
    replacement.id = canvas.id;
    replacement.className = canvas.className;
    replacement.setAttribute('aria-hidden', 'true');
    replacement.style.height = canvas.style.height;
    if (io) {
      io.unobserve(canvas);
    }
    canvas.replaceWith(replacement);
    canvas = replacement;
    if (io) {
      io.observe(canvas);
    }
    ctx = canvas.getContext('2d');
    resize();
  }

  if (glRiver) {
    canvas.addEventListener('webglcontextlost', (ev) => {
      ev.preventDefault();
      fallbackTo2D();
    });
  }

  function measureStageBounds() {
    const bodyRect = document.body.getBoundingClientRect();
    const headerEl = document.querySelector('.site-header');
    const heroEl = document.querySelector('.hero');
    const editorialEl = document.querySelector('.hero-editorial');
    const frame = document.querySelector('.stage-frame');
    if (heroEl) {
      const heroBottom = Math.max(420, Math.round(heroEl.getBoundingClientRect().bottom - bodyRect.top));
      canvas.style.height = `${heroBottom}px`;
    }
    if (headerEl) {
      headerBottom = Math.max(48, Math.round(headerEl.getBoundingClientRect().bottom - bodyRect.top));
    } else {
      headerBottom = 64;
    }
    const widthForNorm = Math.max(1, window.innerWidth || bodyRect.width);
    if (editorialEl) {
      const edRightPx = editorialEl.getBoundingClientRect().right - bodyRect.left + 16;
      editorialRight = Math.min(0.9, Math.max(0.56, edRightPx / widthForNorm));
    } else {
      editorialRight = 0.72;
    }
    baseX = Math.min(0.915, Math.max(0.865, editorialRight + 0.11));
    if (frame) {
      const frameRect = frame.getBoundingClientRect();
      stageTop = Math.max(headerBottom + 60, Math.round(frameRect.top - bodyRect.top));
      stageBottom = Math.max(stageTop + 120, Math.round(frameRect.bottom - bodyRect.top));
    } else {
      stageTop = 210;
      stageBottom = Math.round(cssHeight * 0.88);
    }
  }

  function resize() {
    measureStageBounds();
    const rect = canvas.getBoundingClientRect();
    cssWidth = Math.round(rect.width);
    cssHeight = Math.round(rect.height);
    if (cssWidth < 2 || cssHeight < 2) {
      startOrStop();
      return;
    }
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(cssWidth * dpr);
    canvas.height = Math.round(cssHeight * dpr);
    renderFrame(0);
    startOrStop();
  }

  function smoothstep(edge0, edge1, x) {
    const t = Math.min(1, Math.max(0, (x - edge0) / Math.max(0.0001, edge1 - edge0)));
    return t * t * (3 - 2 * t);
  }

  function render2D(phase, dark) {
    if (!ctx) {
      return;
    }
    const cell = 2;
    const cols = Math.ceil(cssWidth / cell);
    const rows = Math.ceil(cssHeight / cell);

    if (!offscreen || offscreen.width !== cols || offscreen.height !== rows) {
      offscreen = document.createElement('canvas');
      offscreen.width = cols;
      offscreen.height = rows;
      offCtx = offscreen.getContext('2d');
    }

    const imgData = offCtx.createImageData(cols, rows);
    const data = imgData.data;

    const bankDot = dark ? [62, 56, 50] : [210, 206, 196];
    const coreDot = dark ? [178, 102, 72] : [204, 122, 92];

    const invCols = 1 / Math.max(1, cols);
    const invRows = 1 / Math.max(1, rows);

    for (let y = 0; y < rows; y += 1) {
      const ny = y * invRows;
      const cssY = y * cell;
      const headerFade = smoothstep(headerBottom + 6, headerBottom + 54, cssY);
      const bottomFade = 1 - smoothstep(stageBottom - 24, stageBottom + 8, cssY);
      const yMask = headerFade * bottomFade;
      if (yMask <= 0.002) {
        continue;
      }

      const belowEditorial = smoothstep(stageTop - 8, stageTop + 28, cssY);
      const leftGuard = editorialRight + (Math.min(editorialRight, 0.68) - editorialRight) * belowEditorial;
      const [cx, halfW] = riverGeometry(ny, baseX, phase);
      const by = y & 7;
      const rowOffset = y * cols * 4;

      for (let x = 0; x < cols; x += 1) {
        const nx = x * invCols;
        if (nx <= leftGuard) {
          continue;
        }
        const guard = Math.min(1, Math.max(0, (nx - leftGuard) / 0.1));
        const dist = Math.abs((nx - cx) / Math.max(0.01, halfW));
        if (dist >= 1.38) {
          continue;
        }
        const falloff = Math.max(0, 1 - dist / 1.38);
        const density = falloff * falloff * guard * yMask * 0.42;
        const threshold = (BAYER_8X8[by][x & 7] + 0.5) / 64;
        if (density > threshold) {
          const coreMix = Math.pow(Math.max(0, 1 - dist / 0.78), 1.3);
          const r = bankDot[0] + (coreDot[0] - bankDot[0]) * coreMix;
          const g = bankDot[1] + (coreDot[1] - bankDot[1]) * coreMix;
          const b = bankDot[2] + (coreDot[2] - bankDot[2]) * coreMix;
          const alpha = Math.round((0.58 + 0.34 * coreMix) * headerFade * bottomFade * 255);
          const px = rowOffset + x * 4;
          data[px] = r;
          data[px + 1] = g;
          data[px + 2] = b;
          data[px + 3] = alpha;
        }
      }
    }

    offCtx.putImageData(imgData, 0, 0);

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(offscreen, 0, 0, cssWidth, cssHeight);

    ctx.save();
    const clipX = cssWidth * (Math.min(editorialRight, 0.68) + 0.01);
    const clipTop = Math.max(0, headerBottom + 10);
    const clipBottom = Math.min(cssHeight, stageBottom + 2);
    ctx.beginPath();
    ctx.rect(clipX, clipTop, Math.max(0, cssWidth - clipX), Math.max(0, clipBottom - clipTop));
    ctx.clip();
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';

    const accentRgb = dark ? '246, 146, 104' : '183, 69, 37';
    const stoneRgb = dark ? '158, 148, 136' : '126, 120, 110';
    const contours = [
      [-1.02, 0.2, false, 0.08, 0.95],
      [-0.72, 0.65, false, 0.13, 1.0],
      [-0.44, 1.15, false, 0.17, 1.05],
      [-0.2, 1.55, true, 0.22, 1.2],
      [0.0, 0.0, true, 0.34, 1.5],
      [0.21, 2.1, true, 0.22, 1.2],
      [0.46, 2.55, false, 0.17, 1.05],
      [0.75, 3.05, false, 0.12, 1.0],
      [1.06, 3.55, false, 0.08, 0.95],
    ];

    const steps = 56;

    for (const [offset, cPhase, isAccent, alpha, strokeW] of contours) {
      ctx.strokeStyle = `rgba(${isAccent ? accentRgb : stoneRgb}, ${alpha.toFixed(3)})`;
      ctx.lineWidth = strokeW;
      ctx.beginPath();
      for (let i = 0; i <= steps; i += 1) {
        const ny = Math.max(0, (headerBottom + 12) / Math.max(1, cssHeight)) + ((0.96 - (headerBottom + 12) / Math.max(1, cssHeight)) * i) / steps;
        const clamped = Math.min(1, Math.max(0, ny));
        const [cx, halfW] = riverGeometry(clamped, baseX, phase);
        const ripple = 1 + 0.12 * Math.sin(ny * 3 * Math.PI + cPhase + phase * 1.4);
        const x = (cx + offset * halfW * ripple) * cssWidth;
        const y = ny * cssHeight;
        if (i === 0) {
          ctx.moveTo(x, y);
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    }
    ctx.restore();
  }

  function renderFrame(phase) {
    if (cssWidth < 2 || cssHeight < 2) {
      return;
    }
    if (cssWidth <= 760) {
      if (glRiver) {
        glRiver.clear();
      } else if (ctx) {
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, cssWidth, cssHeight);
      }
      return;
    }
    const dark = isDarkScheme();
    if (glRiver) {
      glRiver.render(dpr, phase, dark, headerBottom, stageTop, stageBottom, editorialRight, baseX);
    } else {
      render2D(phase, dark);
    }
  }

  let lastDraw = 0;
  function tick(ts) {
    rafId = 0;
    if (!isVisible || prefersReducedMotion() || document.hidden || cssWidth <= 760) {
      return;
    }
    if (!startTime) {
      startTime = ts;
    }
    // Gentle 12fps drift so the Bayer stipple feels like calm current without GPU churn
    if (ts - lastDraw >= 84) {
      lastDraw = ts;
      const elapsedSec = (ts - startTime) * 0.001;
      renderFrame(elapsedSec * 0.14);
    }
    rafId = window.requestAnimationFrame(tick);
  }

  function startOrStop() {
    if (rafId) {
      window.cancelAnimationFrame(rafId);
      rafId = 0;
    }
    if (cssWidth <= 760) {
      renderFrame(0);
      return;
    }
    if (!prefersReducedMotion() && isVisible && !document.hidden) {
      rafId = window.requestAnimationFrame(tick);
    } else {
      renderFrame(0);
    }
  }

  resize();

  const heroEl = document.querySelector('.hero');
  if (typeof ResizeObserver === 'function') {
    const ro = new ResizeObserver(() => resize());
    if (heroEl) {
      ro.observe(heroEl);
    }
    const headerEl = document.querySelector('.site-header');
    if (headerEl) {
      ro.observe(headerEl);
    }
  }
  window.addEventListener('resize', resize);

  if (typeof IntersectionObserver === 'function') {
    io = new IntersectionObserver((entries) => {
      isVisible = entries.some((entry) => entry.isIntersecting);
      startOrStop();
    });
    io.observe(canvas);
  }

  document.addEventListener('visibilitychange', startOrStop);

  if (window.matchMedia) {
    const darkMq = window.matchMedia('(prefers-color-scheme: dark)');
    if (typeof darkMq.addEventListener === 'function') {
      darkMq.addEventListener('change', () => renderFrame(0));
    }
    const motionMq = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (typeof motionMq.addEventListener === 'function') {
      motionMq.addEventListener('change', startOrStop);
    }
  }
}

function initColorSchemeAndHeroViews() {
  const heroImg = document.getElementById('hero-screenshot');
  const heroSourceDark = document.getElementById('hero-source-dark');
  const heroCaption = document.getElementById('hero-caption');
  const toggleButtons = [...document.querySelectorAll('[data-hero-view]')];
  const tablist = document.querySelector('[role="tablist"]');

  function syncAllPictureLinks(dark) {
    const links = document.querySelectorAll('a.shot-link');
    for (const link of links) {
      const img = link.querySelector('img[data-light][data-dark]');
      if (!img) {
        continue;
      }
      const href = dark ? img.dataset.dark : img.dataset.light;
      if (href) {
        link.setAttribute('href', href);
      }
    }
  }

  function applyHeroView(viewKey) {
    const view = HERO_VIEWS[viewKey] || HERO_VIEWS.sidebar;
    if (heroImg) {
      heroImg.src = view.light;
      heroImg.dataset.light = view.light;
      heroImg.dataset.dark = view.dark;
      heroImg.alt = view.alt;
    }
    if (heroSourceDark) {
      heroSourceDark.srcset = view.dark;
    }
    if (heroCaption) {
      heroCaption.textContent = view.caption;
    }
    for (const btn of toggleButtons) {
      const isSelected = btn.dataset.heroView === viewKey;
      btn.setAttribute('aria-selected', String(isSelected));
      btn.setAttribute('tabindex', isSelected ? '0' : '-1');
    }
    syncAllPictureLinks(isDarkScheme());
  }

  for (const btn of toggleButtons) {
    btn.addEventListener('click', () => {
      applyHeroView(btn.dataset.heroView);
    });
  }

  if (tablist && toggleButtons.length > 1) {
    tablist.addEventListener('keydown', (ev) => {
      const currentIndex = toggleButtons.indexOf(document.activeElement);
      if (currentIndex === -1) {
        return;
      }
      let nextIndex = -1;
      if (ev.key === 'ArrowRight') {
        nextIndex = (currentIndex + 1) % toggleButtons.length;
      } else if (ev.key === 'ArrowLeft') {
        nextIndex = (currentIndex - 1 + toggleButtons.length) % toggleButtons.length;
      } else if (ev.key === 'Home') {
        nextIndex = 0;
      } else if (ev.key === 'End') {
        nextIndex = toggleButtons.length - 1;
      }
      if (nextIndex !== -1) {
        ev.preventDefault();
        const nextBtn = toggleButtons[nextIndex];
        applyHeroView(nextBtn.dataset.heroView);
        nextBtn.focus();
      }
    });
  }

  syncAllPictureLinks(isDarkScheme());
  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    if (typeof mq.addEventListener === 'function') {
      mq.addEventListener('change', (ev) => syncAllPictureLinks(ev.matches));
    }
  }
}

function formatPublishedDate(isoString) {
  if (!isoString) {
    return '';
  }
  const date = new Date(isoString);
  if (Number.isNaN(date.getTime())) {
    return '';
  }
  return new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(date);
}

function formatShortTime(epochMs) {
  const date = new Date(epochMs);
  if (Number.isNaN(date.getTime())) {
    return '';
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function friendlyFormatLabel(format) {
  switch (format) {
    case '.AppImage':
      return 'AppImage';
    case '.deb':
      return 'Debian / Ubuntu (.deb)';
    case '.rpm':
      return 'Fedora / RPM (.rpm)';
    case '.pkg.tar.zst':
      return 'Arch package (.pkg.tar.zst)';
    case '.tar.gz':
      return 'Archive (.tar.gz)';
    default:
      return format;
  }
}

function friendlyArchLabel(arch) {
  switch (arch) {
    case 'x86_64':
      return '64-bit Linux';
    case 'aarch64':
      return 'ARM64 Linux';
    case 'any':
      return 'Linux';
    default:
      return 'Linux';
  }
}

function initDownloadWorkbench() {
  if (!document.getElementById('download-workbench')) {
    return;
  }

  const heroSummaryEl = document.getElementById('hero-release-summary');
  const titleEl = document.getElementById('release-title');
  const metaEl = document.getElementById('release-meta');
  const notesLink = document.getElementById('release-notes-link');
  const noticeEl = document.getElementById('release-notice');
  const readyEl = document.getElementById('release-ready');
  const fallbackEl = document.getElementById('release-fallback');
  const fallbackMsgEl = document.getElementById('fallback-message');
  const selectorRow = document.getElementById('selector-row');
  const archLabel = document.getElementById('arch-label');
  const archWrap = document.getElementById('arch-select-wrap');
  const archSelect = document.getElementById('arch-select');
  const archReadout = document.getElementById('arch-readout');
  const pkgLabel = document.getElementById('package-label');
  const pkgWrap = document.getElementById('package-select-wrap');
  const packageSelect = document.getElementById('package-select');
  const pkgReadout = document.getElementById('package-readout');
  const detailTag = document.getElementById('detail-tag');
  const detailArch = document.getElementById('detail-arch');
  const detailPackage = document.getElementById('detail-package');
  const detailFilename = document.getElementById('detail-filename');
  const availSummary = document.getElementById('availability-summary');
  const reqHeadline = document.getElementById('requirements-headline');
  const reqDetail = document.getElementById('requirements-detail');
  const reqBrief = document.getElementById('download-req-brief');
  const downloadLink = document.getElementById('download-link');
  const assetMeta = document.getElementById('asset-meta');
  const sidecarLink = document.getElementById('sidecar-link');
  const sigLink = document.getElementById('sig-link');
  const digestLine = document.getElementById('digest-line');
  const terminalTitle = document.getElementById('terminal-title');
  const installCommands = document.getElementById('install-commands');
  const commandsNote = document.getElementById('commands-note');
  const copyBtn = document.getElementById('copy-commands');
  const copyStatus = document.getElementById('copy-status');
  const retryBtn = document.getElementById('retry-release');
  const retryFallbackBtn = document.getElementById('retry-release-fallback');

  let currentRelease = null;
  let currentCommandText = '';
  let releaseRequestSeq = 0;
  let copyResetTimer = null;

  function setHeroNeutralLink() {
    if (!heroSummaryEl) {
      return;
    }
    heroSummaryEl.textContent = '';
    const link = document.createElement('a');
    link.className = 'text-link';
    link.href = RELEASES_FALLBACK_URL;
    link.rel = 'noopener noreferrer';
    link.textContent = 'View latest stable release on GitHub';
    heroSummaryEl.appendChild(link);
  }

  function setNotice(message) {
    if (!noticeEl) {
      return;
    }
    if (!message) {
      noticeEl.hidden = true;
      noticeEl.textContent = '';
      return;
    }
    noticeEl.textContent = message;
    noticeEl.hidden = false;
  }

  function syncFieldControl({ count, wrap, readout, label, selectId, singleText }) {
    if (count <= 1) {
      wrap.hidden = true;
      readout.hidden = false;
      readout.textContent = singleText;
      label.removeAttribute('for');
    } else {
      wrap.hidden = false;
      readout.hidden = true;
      label.setAttribute('for', selectId);
    }
  }

  function populateSelectors(release) {
    const arches = [...new Set(release.packages.map((p) => p.arch))];
    const prevArch = archSelect.value;
    archSelect.textContent = '';
    for (const arch of arches) {
      const opt = document.createElement('option');
      opt.value = arch;
      opt.textContent = arch;
      archSelect.appendChild(opt);
    }
    if (arches.includes(prevArch)) {
      archSelect.value = prevArch;
    }
    syncFieldControl({
      count: arches.length,
      wrap: archWrap,
      readout: archReadout,
      label: archLabel,
      selectId: 'arch-select',
      singleText: arches[0] || '',
    });
    syncPackageOptions(release, arches.length);
  }

  function syncPackageOptions(release, archCount) {
    const selectedArch = archSelect.value;
    const matching = release.packages.filter((p) => p.arch === selectedArch);
    const formatCounts = new Map();
    for (const pkg of matching) {
      formatCounts.set(pkg.format, (formatCounts.get(pkg.format) || 0) + 1);
    }

    const prevName = packageSelect.value;
    packageSelect.textContent = '';
    for (const pkg of matching) {
      const opt = document.createElement('option');
      opt.value = pkg.name;
      const hasSibling = (formatCounts.get(pkg.format) || 0) > 1;
      opt.textContent = formatPackageOptionLabel(pkg, hasSibling);
      packageSelect.appendChild(opt);
    }
    if (matching.some((p) => p.name === prevName)) {
      packageSelect.value = prevName;
    }
    const singleLabel = matching[0] ? formatPackageOptionLabel(matching[0], false) : '';
    syncFieldControl({
      count: matching.length,
      wrap: pkgWrap,
      readout: pkgReadout,
      label: pkgLabel,
      selectId: 'package-select',
      singleText: singleLabel,
    });

    if (selectorRow) {
      const totalArches = archCount ?? new Set(release.packages.map((p) => p.arch)).size;
      selectorRow.hidden = totalArches <= 1 && matching.length <= 1;
    }

    renderSelectedPackage(release);
  }

  function renderSelectedPackage(release) {
    const selectedArch = archSelect.value;
    const selectedName = packageSelect.value;
    const pkg =
      release.packages.find((p) => p.name === selectedName && p.arch === selectedArch) ||
      release.packages.find((p) => p.arch === selectedArch) ||
      release.packages[0];

    if (!pkg) {
      return;
    }

    if (detailTag) {
      detailTag.textContent = release.tag;
    }
    if (detailArch) {
      detailArch.textContent = pkg.arch;
    }
    if (detailPackage) {
      detailPackage.textContent = `${pkg.format} (${pkg.sizeLabel})`;
    }
    if (detailFilename) {
      detailFilename.textContent = pkg.name;
    }

    if (release.packages.length === 1) {
      availSummary.textContent = `This release publishes a Linux ${pkg.arch} ${pkg.format} build.`;
    } else {
      const formatList = [...new Set(release.packages.map((p) => p.format))].join(', ');
      availSummary.textContent = `Available package formats in ${release.title}: ${formatList}.`;
    }

    const reqs = getSystemRequirements(release, pkg);
    reqHeadline.textContent = reqs.headline;
    reqDetail.textContent = reqs.detail;

    if (reqBrief) {
      reqBrief.textContent =
        release.version.startsWith('1.1.') && pkg.format === '.AppImage'
          ? 'Wayland and X11. Current AppImages require glibc 2.44 or newer.'
          : `Works on ${friendlyArchLabel(pkg.arch)} with Wayland or X11.`;
    }

    if (heroSummaryEl) {
      heroSummaryEl.textContent = `${release.title} for Linux (${pkg.sizeLabel})`;
    }

    downloadLink.href = pkg.url;
    downloadLink.textContent = `Download for Linux (${pkg.format})`;
    assetMeta.textContent = `${friendlyArchLabel(pkg.arch)} ${friendlyFormatLabel(pkg.format)}, ${pkg.sizeLabel}`;

    if (pkg.sidecar) {
      sidecarLink.href = pkg.sidecar.url;
      sidecarLink.textContent = `Download ${pkg.sidecar.name}`;
      sidecarLink.hidden = false;
    } else {
      sidecarLink.hidden = true;
      sidecarLink.href = RELEASES_FALLBACK_URL;
    }

    if (pkg.sigSidecar) {
      sigLink.href = pkg.sigSidecar.url;
      sigLink.textContent = `External signature: ${pkg.sigSidecar.name} (verify manually with gpg)`;
      sigLink.hidden = false;
    } else {
      sigLink.hidden = true;
      sigLink.href = RELEASES_FALLBACK_URL;
    }

    if (pkg.sha256) {
      digestLine.textContent = `SHA-256: ${pkg.sha256}`;
      digestLine.hidden = false;
    } else {
      digestLine.hidden = true;
      digestLine.textContent = '';
    }

    const cmds = buildInstallCommands(pkg);
    currentCommandText = cmds.lines.join('\n');
    installCommands.textContent = currentCommandText;
    commandsNote.textContent = cmds.note;
    if (terminalTitle) {
      terminalTitle.textContent = cmds.hasVerification
        ? 'Verify download and run in terminal'
        : 'Run in terminal (no checksum published for this asset)';
    }
  }

  function renderResult(result) {
    if (result.status === 'ready' && result.release) {
      const release = result.release;
      currentRelease = release;
      const dateStr = formatPublishedDate(release.publishedAt);
      titleEl.textContent = release.title;
      metaEl.textContent = dateStr ? `Released ${dateStr}` : `Version ${release.tag}`;

      if (notesLink) {
        notesLink.href = release.htmlUrl || RELEASES_FALLBACK_URL;
        notesLink.hidden = false;
      }

      if (result.stale) {
        const resetNote = result.rateLimitResetAt
          ? ` Rate limit resets around ${formatShortTime(result.rateLimitResetAt)}.`
          : '';
        setNotice(`${result.errorDetail}${resetNote}`);
      } else if (result.fromCache && result.fetchedAt) {
        setNotice(`Showing saved release info from ${formatShortTime(result.fetchedAt)}. Use Refresh to check GitHub now.`);
      } else {
        setNotice('');
      }

      fallbackEl.hidden = true;
      readyEl.hidden = false;
      populateSelectors(release);
      return;
    }

    setHeroNeutralLink();
    readyEl.hidden = true;
    fallbackEl.hidden = false;
    setNotice('');

    if (result.status === 'empty' && result.release) {
      titleEl.textContent = result.release.title;
      metaEl.textContent = `Version ${result.release.tag}`;
      if (notesLink) {
        notesLink.href = result.release.htmlUrl || RELEASES_FALLBACK_URL;
        notesLink.hidden = false;
      }
      fallbackMsgEl.textContent =
        'This release does not include prebuilt Linux packages (.AppImage, .deb, .rpm, .pkg.tar.zst, or .tar.gz). Check the release notes on GitHub or build from source.';
      return;
    }

    if (notesLink) {
      notesLink.hidden = true;
    }
    titleEl.textContent = 'Direct lookup unavailable';
    metaEl.textContent = 'Could not check GitHub right now';

    if (result.errorKind === 'ratelimit') {
      const resetNote = result.rateLimitResetAt
        ? ` GitHub indicates the limit resets around ${formatShortTime(result.rateLimitResetAt)}.`
        : '';
      fallbackMsgEl.textContent = `GitHub API rate limit reached for this connection.${resetNote} Download directly from the official GitHub Releases page.`;
    } else if (result.errorKind === 'timeout') {
      fallbackMsgEl.textContent =
        'The GitHub request timed out after 8 seconds. Try again or open the official GitHub Releases page directly.';
    } else {
      fallbackMsgEl.textContent =
        'Unable to reach GitHub right now. Open the official GitHub Releases page directly or try again later.';
    }
  }

  async function loadRelease({ bypassCache = false } = {}) {
    const reqSeq = ++releaseRequestSeq;
    const cached = bypassCache ? null : readReleaseCache(window.localStorage);

    if (cached && cached.fresh && cached.release.packages.length > 0) {
      renderResult({
        status: 'ready',
        release: cached.release,
        stale: false,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorKind: null,
        errorDetail: null,
      });
      return;
    }

    if (cached && cached.release.packages.length > 0) {
      renderResult({
        status: 'ready',
        release: cached.release,
        stale: true,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorDetail: `Showing saved release info from ${formatShortTime(cached.fetchedAt)} while checking GitHub.`,
      });
    } else {
      titleEl.textContent = 'Latest stable release';
      metaEl.textContent = 'Checking GitHub for the latest build';
      setNotice('');
      readyEl.hidden = true;
      fallbackEl.hidden = true;
    }

    const fresh = await fetchLatestRelease({
      storage: window.localStorage,
      bypassCache,
    });
    if (reqSeq !== releaseRequestSeq) {
      return;
    }
    renderResult(fresh);
  }

  archSelect.addEventListener('change', () => {
    if (currentRelease) {
      syncPackageOptions(currentRelease);
    }
  });

  packageSelect.addEventListener('change', () => {
    if (currentRelease) {
      renderSelectedPackage(currentRelease);
    }
  });

  if (retryBtn) {
    retryBtn.addEventListener('click', () => {
      loadRelease({ bypassCache: true });
    });
  }

  if (retryFallbackBtn) {
    retryFallbackBtn.addEventListener('click', () => {
      loadRelease({ bypassCache: true });
    });
  }

  if (copyBtn) {
    copyBtn.addEventListener('click', async () => {
      if (!currentCommandText) {
        return;
      }
      try {
        await navigator.clipboard.writeText(currentCommandText);
        copyBtn.textContent = 'Copied';
        copyStatus.textContent = 'Commands copied to clipboard.';
        if (copyResetTimer !== null) {
          clearTimeout(copyResetTimer);
        }
        copyResetTimer = setTimeout(() => {
          copyBtn.textContent = 'Copy commands';
          copyResetTimer = null;
        }, 1800);
      } catch {
        copyStatus.textContent = 'Clipboard copy unavailable; select the command text manually.';
      }
    });
  }

  loadRelease();
}

function initScreenshotLightbox() {
  const dialog = document.getElementById('screenshot-lightbox');
  const stage = document.getElementById('lightbox-stage');
  const shotLinks = [...document.querySelectorAll('[data-shot-link]')];

  if (!dialog || typeof dialog.showModal !== 'function' || !stage || shotLinks.length === 0) {
    return;
  }

  const groupEl = document.getElementById('lightbox-group');
  const counterEl = document.getElementById('lightbox-counter');
  const rawLink = document.getElementById('lightbox-raw-link');
  const titleEl = document.getElementById('lightbox-title');
  const descEl = document.getElementById('lightbox-desc');
  const closeBtn = document.getElementById('lightbox-close');
  const prevBtn = document.getElementById('lightbox-prev');
  const nextBtn = document.getElementById('lightbox-next');

  let activeIndex = 0;
  let lastTrigger = null;

  function resolveShotHref(link, thumb) {
    if (thumb) {
      const dark = isDarkScheme();
      if (dark && thumb.dataset.dark) {
        return thumb.dataset.dark;
      }
      if (!dark && thumb.dataset.light) {
        return thumb.dataset.light;
      }
    }
    return link.getAttribute('href') || '';
  }

  function renderAt(index) {
    const len = shotLinks.length;
    activeIndex = ((index % len) + len) % len;
    const link = shotLinks[activeIndex];
    const thumb = link.querySelector('img');
    const figure = link.closest('figure');
    const section = link.closest('[data-shot-group]');
    const titleNode = figure ? figure.querySelector('.shot-title') : null;
    const descNode = figure ? figure.querySelector('.shot-desc') : null;
    const stageCaptionNode = figure ? figure.querySelector('.stage-caption') : null;
    const plateCaptionNode = figure ? figure.querySelector('.plate-caption') : null;
    const href = resolveShotHref(link, thumb);

    stage.textContent = '';
    const fullImg = document.createElement('img');
    fullImg.className = 'lightbox-img';
    fullImg.src = href;
    fullImg.alt = thumb ? thumb.alt : '';
    if (thumb && thumb.width > 0 && thumb.height > 0) {
      fullImg.width = thumb.width;
      fullImg.height = thumb.height;
    }
    stage.appendChild(fullImg);

    if (groupEl) {
      groupEl.textContent = link.dataset.shotGroup || section?.dataset.shotGroup || 'Rill screenshot';
    }
    if (counterEl) {
      counterEl.textContent = `${activeIndex + 1} / ${len}`;
    }
    if (rawLink) {
      rawLink.href = href;
    }
    if (titleEl) {
      if (link.id === 'hero-shot-link' && stageCaptionNode) {
        titleEl.textContent = stageCaptionNode.textContent.trim();
      } else if (link.dataset.shotTitle) {
        titleEl.textContent = link.dataset.shotTitle;
      } else if (titleNode) {
        titleEl.textContent = titleNode.textContent.trim();
      } else {
        titleEl.textContent = 'Screenshot';
      }
    }
    if (descEl) {
      if (descNode) {
        descEl.textContent = descNode.textContent.trim().replace(/\s+/g, ' ');
      } else if (plateCaptionNode) {
        descEl.textContent = plateCaptionNode.textContent.trim().replace(/\s+/g, ' ');
      } else if (link.id === 'hero-shot-link' && thumb) {
        descEl.textContent = thumb.alt;
      } else {
        descEl.textContent = '';
      }
    }
    if (prevBtn) {
      prevBtn.disabled = len <= 1;
    }
    if (nextBtn) {
      nextBtn.disabled = len <= 1;
    }
  }

  for (let i = 0; i < shotLinks.length; i += 1) {
    const link = shotLinks[i];
    link.addEventListener('click', (ev) => {
      if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey || ev.button !== 0) {
        return;
      }
      ev.preventDefault();
      lastTrigger = link;
      renderAt(i);
      if (!dialog.open) {
        dialog.showModal();
      }
    });
  }

  if (closeBtn) {
    closeBtn.addEventListener('click', () => dialog.close());
  }
  if (prevBtn) {
    prevBtn.addEventListener('click', () => renderAt(activeIndex - 1));
  }
  if (nextBtn) {
    nextBtn.addEventListener('click', () => renderAt(activeIndex + 1));
  }

  dialog.addEventListener('click', (ev) => {
    if (ev.target === dialog) {
      dialog.close();
    }
  });

  dialog.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowLeft') {
      ev.preventDefault();
      renderAt(activeIndex - 1);
    } else if (ev.key === 'ArrowRight') {
      ev.preventDefault();
      renderAt(activeIndex + 1);
    }
  });

  dialog.addEventListener('close', () => {
    stage.textContent = '';
    if (lastTrigger && typeof lastTrigger.focus === 'function') {
      lastTrigger.focus();
    }
    lastTrigger = null;
  });

  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    if (typeof mq.addEventListener === 'function') {
      mq.addEventListener('change', () => {
        if (dialog.open) {
          renderAt(activeIndex);
        }
      });
    }
  }
}

function init() {
  initRiverCanvas();
  initColorSchemeAndHeroViews();
  initDownloadWorkbench();
  initScreenshotLightbox();
}

init();
