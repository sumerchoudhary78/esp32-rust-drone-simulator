import * as THREE from "/vendor/three.module.js";

// The scene displays server truth. Flight dynamics and stabilization run in Rust.
export function createScene(canvas) {
  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    alpha: false,
  });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.setClearColor(0xf7f9f6);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.2;
  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(0xf7f9f6, 2.5, 10);
  const camera = new THREE.PerspectiveCamera(34, 1, 0.01, 80);
  scene.add(new THREE.HemisphereLight(0xfafff5, 0x8b947e, 3));
  const sun = new THREE.DirectionalLight(0xfff9ee, 3.1);
  sun.position.set(2, 4, 1);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  sun.shadow.camera.left = -3;
  sun.shadow.camera.right = 3;
  sun.shadow.camera.top = 5;
  sun.shadow.camera.bottom = -3;
  sun.shadow.normalBias = 0.003;
  sun.shadow.bias = -0.0001;
  scene.add(sun);
  const rim = new THREE.DirectionalLight(0xdde8ff, 1.4);
  rim.position.set(-2, 1, -2);
  scene.add(rim);
  const ground = new THREE.Mesh(
    new THREE.PlaneGeometry(100, 100),
    new THREE.MeshStandardMaterial({ color: 0xf2f5ef, roughness: 1 }),
  );
  ground.rotation.x = -Math.PI / 2;
  ground.position.y = -0.047;
  ground.receiveShadow = true;
  scene.add(ground);
  const grid = new THREE.GridHelper(30, 120, 0xc4cfbc, 0xdfe5d9);
  grid.position.y = -0.046;
  grid.material.transparent = true;
  grid.material.opacity = 0.65;
  scene.add(grid);
  function ring(radius, color, opacity) {
    const points = Array.from(
      { length: 128 },
      (_, i) =>
        new THREE.Vector3(
          Math.cos((i / 128) * Math.PI * 2) * radius,
          -0.044,
          Math.sin((i / 128) * Math.PI * 2) * radius,
        ),
    );
    const line = new THREE.LineLoop(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.LineBasicMaterial({ color, transparent: true, opacity }),
    );
    scene.add(line);
  }
  ring(0.36, 0xa8b897, 0.5);
  ring(0.39, 0xb3c3a2, 0.25);
  const marker = new THREE.Group();
  const markerMaterial = new THREE.MeshBasicMaterial({ color: 0xc7d1bb });
  for (const angle of [0, Math.PI / 2, Math.PI, Math.PI * 1.5]) {
    const tick = new THREE.Mesh(
      new THREE.BoxGeometry(0.025, 0.001, 0.003),
      markerMaterial,
    );
    tick.position.set(Math.sin(angle) * 0.42, -0.043, Math.cos(angle) * 0.42);
    tick.rotation.y = -angle;
    marker.add(tick);
  }
  scene.add(marker);
  const drone = new THREE.Group();
  scene.add(drone);
  const mat = (color, roughness = 0.65, metalness = 0.15) =>
    new THREE.MeshStandardMaterial({ color, roughness, metalness });
  const carbon = mat(0x2b3233),
    black = mat(0x20282b),
    orange = mat(0xe35e39),
    white = mat(0xe2e5dc),
    metal = mat(0xa7b2aa, 0.35, 0.7);
  function mesh(geometry, material, x, y, z, parent = drone) {
    const m = new THREE.Mesh(geometry, material);
    m.position.set(x, y, z);
    m.castShadow = true;
    m.receiveShadow = true;
    parent.add(m);
    return m;
  }
  function box(w, h, d, material, x = 0, y = 0, z = 0, parent = drone) {
    return mesh(new THREE.BoxGeometry(w, h, d), material, x, y, z, parent);
  }
  function cylinder(r1, r2, h, material, x, y, z, parent = drone) {
    return mesh(
      new THREE.CylinderGeometry(r1, r2, h, 24),
      material,
      x,
      y,
      z,
      parent,
    );
  }
  box(0.119, 0.008, 0.155, carbon, 0, 0.03, 0);
  box(0.128, 0.007, 0.15, carbon, 0, 0.075, 0);
  for (const x of [-0.047, 0.047])
    for (const z of [-0.058, 0.058]) {
      cylinder(0.003, 0.003, 0.038, orange, x, 0.052, z);
      cylinder(0.003, 0.003, 0.002, metal, x, 0.08, z);
    }
  // Battery, regulator, ESP32 module and six-axis IMU board.
  box(0.06, 0.026, 0.105, mat(0x3f4740), 0, 0.092, 0.012);
  box(0.062, 0.003, 0.02, black, 0, 0.107, 0.022);
  box(0.034, 0.004, 0.052, mat(0x3c7663), 0, 0.114, -0.017);
  box(0.027, 0.005, 0.025, metal, 0, 0.118, -0.008);
  box(0.025, 0.002, 0.014, black, 0, 0.118, -0.038);
  box(0.014, 0.004, 0.018, mat(0x548e9b), 0.044, 0.082, -0.014);
  cylinder(0.004, 0.004, 0.003, mat(0x78bb7b), -0.038, 0.082, -0.045);
  const wireCurve = new THREE.CatmullRomCurve3([
    new THREE.Vector3(0.025, 0.09, 0.055),
    new THREE.Vector3(0.08, 0.115, 0.037),
    new THREE.Vector3(0.074, 0.053, -0.03),
    new THREE.Vector3(0.02, 0.043, -0.035),
  ]);
  mesh(new THREE.TubeGeometry(wireCurve, 24, 0.002, 6, false), orange, 0, 0, 0);
  const rotors = [],
    blurs = [];
  const positions = [
    [-1, -1],
    [1, -1],
    [1, 1],
    [-1, 1],
  ];
  const bladeShape = new THREE.Shape();
  bladeShape.moveTo(0, -0.004);
  bladeShape.bezierCurveTo(0.04, -0.009, 0.11, -0.004, 0.118, 0.001);
  bladeShape.bezierCurveTo(0.12, 0.014, 0.065, 0.024, 0.025, 0.011);
  bladeShape.lineTo(0, 0.004);
  bladeShape.closePath();
  const bladeGeometry = new THREE.ShapeGeometry(bladeShape, 16);
  for (let i = 0; i < 4; i++) {
    const [sx, sz] = positions[i],
      x = sx * 0.159099,
      z = sz * 0.159099;
    const armMaterial = sz < 0 ? orange : white;
    const arm = box(0.029, 0.014, 0.224, armMaterial, x / 2, 0.041, z / 2);
    arm.rotation.y = Math.atan2(x, z);
    const rib = box(0.012, 0.014, 0.196, armMaterial, x / 2, 0.055, z / 2);
    rib.rotation.y = Math.atan2(x, z);
    cylinder(0.023, 0.023, 0.006, carbon, x, 0.052, z);
    cylinder(0.017, 0.019, 0.026, black, x, 0.071, z);
    cylinder(0.019, 0.017, 0.011, orange, x, 0.085, z);
    cylinder(0.005, 0.005, 0.018, metal, x, 0.094, z);
    for (let a = 0; a < 8; a++) {
      const fin = box(
        0.002,
        0.011,
        0.007,
        black,
        x + Math.sin((a * Math.PI) / 4) * 0.017,
        0.085,
        z + Math.cos((a * Math.PI) / 4) * 0.017,
      );
      fin.rotation.y = (a * Math.PI) / 4;
    }
    const rotor = new THREE.Group();
    rotor.position.set(x, 0.102, z);
    rotor.rotation.y = i * 0.9;
    drone.add(rotor);
    rotors.push(rotor);
    const propMat = new THREE.MeshStandardMaterial({
      color: sz < 0 ? 0x777e78 : 0x8a9486,
      side: THREE.DoubleSide,
      roughness: 0.45,
      metalness: 0.1,
      transparent: true,
      opacity: 0.82,
    });
    for (const angle of [0, Math.PI]) {
      const blade = mesh(bladeGeometry, propMat, 0, 0, 0, rotor);
      blade.rotation.x = -Math.PI / 2;
      blade.rotation.z = angle;
    }
    cylinder(0.007, 0.007, 0.006, metal, 0, 0.003, 0, rotor);
    const blur = new THREE.Mesh(
      new THREE.CircleGeometry(0.121, 48),
      new THREE.MeshBasicMaterial({
        color: 0x97a18e,
        transparent: true,
        opacity: 0,
        side: THREE.DoubleSide,
        depthWrite: false,
      }),
    );
    blur.rotation.x = -Math.PI / 2;
    blur.position.set(x, 0.102, z);
    drone.add(blur);
    blurs.push(blur);
    // Short landing feet with black rubber pads.
    box(0.01, 0.075, 0.012, armMaterial, x * 0.72, 0.004, z * 0.72);
    box(0.022, 0.008, 0.03, black, x * 0.72, -0.039, z * 0.72);
  }
  const trailData = new Float32Array(3000 * 3);
  const trailGeometry = new THREE.BufferGeometry();
  trailGeometry.setAttribute(
    "position",
    new THREE.BufferAttribute(trailData, 3),
  );
  trailGeometry.setDrawRange(0, 0);
  const trail = new THREE.Line(
    trailGeometry,
    new THREE.LineBasicMaterial({
      color: 0xd69770,
      transparent: true,
      opacity: 0.5,
    }),
  );
  trail.frustumCulled = false;
  scene.add(trail);
  const aim = new THREE.Vector3(0, 0.07, 0),
    targetPosition = new THREE.Vector3(),
    targetQuaternion = new THREE.Quaternion();
  let latest = null,
    theta = 0.72,
    phi = 1.02,
    distance = 1.23,
    cameraMode = "orbit",
    lastTick = 0,
    trailCount = 0;
  function update(telemetry) {
    latest = telemetry;
    const p = telemetry.position_m,
      q = telemetry.orientation;
    targetPosition.set(-p.y, p.z, -p.x);
    targetQuaternion.set(-q.y, q.z, -q.x, q.w);
    if (telemetry.tick < lastTick) {
      trailCount = 0;
      trailGeometry.setDrawRange(0, 0);
      drone.position.copy(targetPosition);
    }
    lastTick = telemetry.tick;
    if (p.z > 0.05 && trailCount < 3000) {
      trailData.set(
        [targetPosition.x, targetPosition.y, targetPosition.z],
        trailCount++ * 3,
      );
      trailGeometry.attributes.position.needsUpdate = true;
      trailGeometry.setDrawRange(0, trailCount);
    }
  }
  let dragging = false,
    prevX = 0,
    prevY = 0;
  canvas.addEventListener("pointerdown", (e) => {
    dragging = true;
    prevX = e.clientX;
    prevY = e.clientY;
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    theta -= (e.clientX - prevX) * 0.008;
    phi = Math.max(0.18, Math.min(1.48, phi + (e.clientY - prevY) * 0.006));
    prevX = e.clientX;
    prevY = e.clientY;
    cameraMode = "orbit";
  });
  for (const event of ["pointerup", "pointercancel", "lostpointercapture"])
    canvas.addEventListener(event, () => {
      dragging = false;
    });
  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      distance = Math.max(
        0.75,
        Math.min(5, distance * (e.deltaY > 0 ? 1.08 : 0.92)),
      );
    },
    { passive: false },
  );
  new ResizeObserver(() => {
    const r = canvas.getBoundingClientRect();
    if (r.width && r.height) {
      renderer.setSize(r.width, r.height, false);
      camera.aspect = r.width / r.height;
      camera.updateProjectionMatrix();
    }
  }).observe(canvas);
  let lastFrame = performance.now();
  function render() {
    requestAnimationFrame(render);
    if (document.hidden || canvas.getBoundingClientRect().width === 0) return;
    const now = performance.now(),
      dt = Math.min((now - lastFrame) / 1000, 0.05);
    lastFrame = now;
    drone.position.lerp(targetPosition, 1 - Math.exp(-dt * 18));
    drone.quaternion.slerp(targetQuaternion, 1 - Math.exp(-dt * 20));
    aim.lerp(
      drone.position.clone().add(new THREE.Vector3(0, 0.04, 0)),
      1 - Math.exp(-dt * 9),
    );
    if (cameraMode === "top")
      camera.position.copy(aim).add(new THREE.Vector3(0.001, distance, 0.001));
    else if (cameraMode === "follow")
      camera.position
        .copy(aim)
        .add(
          new THREE.Vector3(0, distance * 0.45, distance * 0.9).applyQuaternion(
            drone.quaternion,
          ),
        );
    else
      camera.position
        .copy(aim)
        .add(
          new THREE.Vector3(
            Math.sin(theta) * Math.sin(phi),
            Math.cos(phi),
            Math.cos(theta) * Math.sin(phi),
          ).multiplyScalar(distance),
        );
    camera.lookAt(aim);
    for (let i = 0; i < 4; i++) {
      const speed = latest?.motor_actual[i] || 0;
      rotors[i].rotation.y += dt * (i % 2 ? -1 : 1) * speed * 100;
      blurs[i].material.opacity = speed * 0.14;
    }
    renderer.render(scene, camera);
  }
  render();
  return {
    update,
    setCamera(mode) {
      cameraMode = mode;
    },
  };
}
