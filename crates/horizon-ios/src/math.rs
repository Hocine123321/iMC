//! The little matrix math the spinning cube needs. Column-major, the way WGSL's `mat4x4` is laid
//! out, with clip-space depth 0..1 (wgpu, Metal).

pub type Mat4 = [[f32; 4]; 4];

pub const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `a * b`: `b` is applied first.
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ]
}

pub fn translation(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = IDENTITY;
    m[3] = [x, y, z, 1.0];
    m
}

pub fn rotation_y(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

pub fn rotation_x(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, c, s, 0.0],
        [0.0, -s, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The matrix as the bytes of a uniform buffer.
pub fn to_bytes(m: &Mat4) -> Vec<u8> {
    m.iter().flatten().flat_map(|v| v.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_changes_nothing() {
        let m = mul(&rotation_y(0.7), &translation(1.0, 2.0, 3.0));
        assert_eq!(mul(&IDENTITY, &m), m);
        assert_eq!(mul(&m, &IDENTITY), m);
    }

    #[test]
    fn a_translation_moves_the_origin() {
        let m = translation(1.0, 2.0, 3.0);
        assert_eq!(m[3], [1.0, 2.0, 3.0, 1.0]);
    }

    #[test]
    fn a_quarter_turn_about_y_takes_x_to_minus_z() {
        let m = rotation_y(std::f32::consts::FRAC_PI_2);
        let x = [m[0][0], m[0][1], m[0][2]];
        assert!(x[0].abs() < 1e-6 && x[1].abs() < 1e-6 && (x[2] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_uniform_is_sixteen_floats() {
        assert_eq!(to_bytes(&IDENTITY).len(), 64);
    }
}
