"""Face orientation checks for Blender source meshes, before edge splitting/export.

Open shells (eyelids, domes) are allowed. Closed islands must enclose positive
signed volume; a whole-model volume would hide a small inverted part in a body.
"""

import bmesh


def face_islands(bm):
    remaining = set(bm.faces)
    while remaining:
        seed = remaining.pop()
        island, pending = {seed}, [seed]
        while pending:
            face = pending.pop()
            for edge in face.edges:
                for other in edge.link_faces:
                    if other in remaining:
                        remaining.remove(other)
                        island.add(other)
                        pending.append(other)
        yield island


def closed(island):
    return all(e.is_manifold for f in island for e in f.edges)


def signed_volume(island):
    # Subtract a nearby origin to keep translated small parts precise.
    origin = next(iter(island)).verts[0].co.copy()
    volume = 0.0
    for face in island:
        vertices = [v.co - origin for v in face.verts]
        for i in range(1, len(vertices) - 1):
            volume += vertices[0].dot(vertices[i].cross(vertices[i + 1])) / 6
    return volume


def orientation_problems(bm):
    problems = []
    nonmanifold = sum(len(e.link_faces) > 2 for e in bm.edges)
    if nonmanifold:
        problems.append(f"{nonmanifold} edges shared by more than two faces")
    inconsistent = sum(not e.is_contiguous for e in bm.edges if e.is_manifold)
    if inconsistent:
        problems.append(f"{inconsistent} edges with inconsistent face winding")
    for island in face_islands(bm):
        if not closed(island):
            continue
        volume = signed_volume(island)
        vertices = {v for f in island for v in f.verts}
        extent = max(max(v.co[i] for v in vertices) - min(v.co[i] for v in vertices) for i in range(3))
        tolerance = extent ** 3 * 1e-10
        if volume < -tolerance:
            problems.append(f"inverted closed island ({len(island)} faces, volume {volume:.6g} m³)")
        elif abs(volume) <= tolerance:
            problems.append(f"zero-volume closed island ({len(island)} faces)")
    return problems


def require_orientation(bm, name):
    problems = orientation_problems(bm)
    if problems:
        raise ValueError(f"{name}: " + "; ".join(problems))


def require_mesh_orientation(mesh):
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        require_orientation(bm, mesh.name)
    finally:
        bm.free()


def orient_mesh(mesh, *, connected=False):
    """Repair generated closed surfaces before modifiers consume their normals."""
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
        # Blender's consistency pass can still choose the inward side of a
        # small detached Skin island. Decide outside per closed island.
        for island in face_islands(bm):
            if closed(island) and signed_volume(island) < 0:
                bmesh.ops.reverse_faces(bm, faces=list(island))
        require_orientation(bm, mesh.name)
        if connected and sum(1 for _ in face_islands(bm)) != 1:
            raise ValueError(f"{mesh.name}: generated body must be one connected surface")
        bm.to_mesh(mesh)
        mesh.update()
    finally:
        bm.free()
