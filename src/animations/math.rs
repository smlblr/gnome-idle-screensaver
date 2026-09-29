//! Column-major 4×4 matrices with the exact semantics of the fixed-function calls
//! the ported demos were written against (glTranslatef, glRotatef, glFrustum).

use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    pub fn translation(x: f32, y: f32, z: f32) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.0[12] = x;
        m.0[13] = y;
        m.0[14] = z;
        m
    }

    /// glRotatef(angle_deg, x, y, z).
    pub fn rotation(angle_deg: f32, x: f32, y: f32, z: f32) -> Mat4 {
        let len = (x * x + y * y + z * z).sqrt();
        let (x, y, z) = (x / len, y / len, z / len);
        let (s, c) = angle_deg.to_radians().sin_cos();
        let t = 1.0 - c;
        Mat4([
            x * x * t + c,
            y * x * t + z * s,
            x * z * t - y * s,
            0.0,
            x * y * t - z * s,
            y * y * t + c,
            y * z * t + x * s,
            0.0,
            x * z * t + y * s,
            y * z * t - x * s,
            z * z * t + c,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }

    /// glFrustum(left, right, bottom, top, near, far).
    pub fn frustum(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> Mat4 {
        let mut m = [0.0; 16];
        m[0] = 2.0 * n / (r - l);
        m[5] = 2.0 * n / (t - b);
        m[8] = (r + l) / (r - l);
        m[9] = (t + b) / (t - b);
        m[10] = -(f + n) / (f - n);
        m[11] = -1.0;
        m[14] = -2.0 * f * n / (f - n);
        Mat4(m)
    }

    /// The normal matrix, valid only while the matrix is rigid (rotations and translations).
    pub fn rotation_part(&self) -> [f32; 9] {
        let m = &self.0;
        [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
    }
}

impl Mul for Mat4 {
    type Output = Mat4;

    fn mul(self, o: Mat4) -> Mat4 {
        let (a, b) = (&self.0, &o.0);
        let mut r = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                r[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
            }
        }
        Mat4(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
        let m = &m.0;
        [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn rotation_is_counter_clockwise_like_gl_rotatef() {
        assert!(close(
            apply(&Mat4::rotation(90.0, 0.0, 0.0, 1.0), [1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0]
        ));
        assert!(close(
            apply(&Mat4::rotation(90.0, 1.0, 0.0, 0.0), [0.0, 1.0, 0.0]),
            [0.0, 0.0, 1.0]
        ));
    }

    #[test]
    fn product_applies_right_operand_first() {
        let m = Mat4::translation(1.0, 0.0, 0.0) * Mat4::rotation(90.0, 0.0, 0.0, 1.0);
        assert!(close(apply(&m, [1.0, 0.0, 0.0]), [1.0, 1.0, 0.0]));
    }
}
