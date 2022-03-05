#![feature(result_option_inspect)]

use core::ops;
use glam::{vec3, vec4, Vec3, Vec4};
use rand_distr::Distribution;

#[derive(Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Ray { origin, direction }
    }

    pub fn at(self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

pub struct World {
    spheres: Vec<Sphere>,
}

impl Default for World {
    fn default() -> Self {
        World {
            spheres: vec![
                Sphere {
                    center: vec3(0., -100.5, -1.),
                    radius: 100.,
                },
                Sphere {
                    center: vec3(0., 0., -1.),
                    radius: 0.5,
                },
            ],
        }
    }
}

impl World {
    pub fn color(&self, rng: &mut rand_pcg::Pcg32, ray: Ray, depth: u32) -> Vec4 {
        if depth <= 0 {
            return vec4(0., 0., 0., 1.);
        }

        let init_t_range = ops::Range {
            start: 0.001,
            end: f32::INFINITY,
        };
        let hit = match ray.hit(self, init_t_range) {
            Some(h) => h,
            None => {
                let t = 0.5 * (ray.direction.normalize_or_zero().y + 1.);
                return Vec4::ONE.lerp(vec4(0.25, 0.49, 1.0, 1.0), t);
            }
        };

        let direction = hit.normal + Vec3::from(rand_distr::UnitSphere.sample(rng));
        let next = Ray {
            origin: hit.at,
            direction,
        };
        0.5 * self.color(rng, next, depth - 1)
    }
}

trait Hit {
    fn hit_with_ray(&self, ray: Ray, t_r: ops::Range<f32>) -> Option<HitReport>;
}

#[derive(Clone, Copy)]
struct HitReport {
    at: Vec3,
    t: f32,
    normal: Vec3,
    face: Face,
}

#[derive(Clone, Copy)]
enum Face {
    Front,
    Back,
}

impl ops::Neg for Face {
    type Output = Self;

    fn neg(self) -> Self {
        match self {
            Face::Front => Face::Back,
            Face::Back => Face::Front,
        }
    }
}

impl HitReport {
    fn correct_face(mut self, ray: Ray) -> Self {
        if self.normal.dot(ray.direction) > 0. {
            self.normal = -self.normal;
            self.face = -self.face;
        }
        self
    }
}

impl Ray {
    fn hit(self, visible: &impl Hit, t_r: ops::Range<f32>) -> Option<HitReport> {
        visible.hit_with_ray(self, t_r)
    }
}

impl Hit for World {
    fn hit_with_ray(&self, ray: Ray, mut t_r: ops::Range<f32>) -> Option<HitReport> {
        let mut hit = None;
        for s in &self.spheres {
            if let Some(h) = ray.hit(s, t_r.clone()) {
                hit = Some(h);
                t_r.end = h.t;
            }
        }
        hit
    }
}

#[derive(Clone, Copy)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Hit for Sphere {
    fn hit_with_ray(&self, ray: Ray, t_r: ops::Range<f32>) -> Option<HitReport> {
        let oc = ray.origin - self.center;
        let a = ray.direction.length_squared();
        let b = oc.dot(ray.direction);
        let c = oc.length_squared() - self.radius.powi(2);
        let d = b.powi(2) - a * c;

        let t = (d >= 0.)
            .then(|| (-b - d.sqrt()) / a)
            .filter(|t| t_r.contains(t))?;
        let at = ray.at(t);
        let normal = (at - self.center) / self.radius;

        Some(
            HitReport {
                t,
                at,
                normal,
                face: Face::Front,
            }
            .correct_face(ray),
        )
    }
}
