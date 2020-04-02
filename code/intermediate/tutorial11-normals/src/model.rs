use std::path::Path;
use std::ops::Range;

use crate::texture;

pub trait Vertex {
    fn desc<'a>() -> wgpu::VertexBufferDescriptor<'a>;
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ModelVertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    normal: [f32; 3],
}

impl Vertex for ModelVertex {
    fn desc<'a>() -> wgpu::VertexBufferDescriptor<'a> {
        use std::mem;
        wgpu::VertexBufferDescriptor {
            stride: mem::size_of::<ModelVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttributeDescriptor {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float3,
                },
                wgpu::VertexAttributeDescriptor {
                    offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float2,
                },
                wgpu::VertexAttributeDescriptor {
                    offset: mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float3,
                },
            ]
        }
    }
}

pub struct Material {
    pub name: String,
    pub diffuse_texture: texture::Texture,
    pub normal_texture: texture::Texture,
    pub bind_group: wgpu::BindGroup,
}

pub struct Mesh {
    pub name: String,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_elements: u32,
    pub material: usize,
}

pub struct Model {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}


impl Model {
    pub fn load<P: AsRef<Path>>(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, path: P) -> Result<(Self, Vec<wgpu::CommandBuffer>), failure::Error> {
        let (obj_models, obj_materials) = tobj::load_obj(path.as_ref())?;

        // We're assuming that the texture files are stored with the obj file        
        let containing_folder = path.as_ref().parent().unwrap();

        // Our `Texure` struct currently returns a `CommandBuffer` when it's created so we need to collect those and return them.
        let mut command_buffers = Vec::new();

        let mut materials = Vec::new();
        for mat in obj_materials {
            let diffuse_path = mat.diffuse_texture;
            let (diffuse_texture, cmds) = texture::Texture::load(&device, containing_folder.join(diffuse_path))?;
            command_buffers.push(cmds);

            let normal_path = match mat.normal_texture.as_str() {
                "" => {
                    // Different modeling software can store objs differently, so tobj stores material parameters
                    // it's not familiar with in a HashMap
                    &mat.unknown_param["map_Bump"]
                }
                path => path,
            };
            let (normal_texture, cmds) = texture::Texture::load(&device, containing_folder.join(normal_path))?;
            command_buffers.push(cmds);
            
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout,
                bindings: &[
                    wgpu::Binding {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                    },
                    wgpu::Binding {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                    },
                    // Normal map bindings
                    wgpu::Binding {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&normal_texture.view),
                    },
                    wgpu::Binding {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&normal_texture.sampler),
                    },
                ]
            });

            materials.push(Material {
                name: mat.name,
                diffuse_texture,
                normal_texture,
                bind_group,
            });
        }

        let mut meshes = Vec::new();
        for m in obj_models {
            let mut vertices = Vec::new();
            for i in 0..m.mesh.positions.len() / 3 {
                vertices.push(ModelVertex {
                    position: [
                        m.mesh.positions[i * 3],
                        m.mesh.positions[i * 3 + 1],
                        m.mesh.positions[i * 3 + 2],
                    ],
                    tex_coords: [
                        m.mesh.texcoords[i * 2],
                        m.mesh.texcoords[i * 2 + 1],
                    ],
                    normal: [
                        m.mesh.normals[i * 3],
                        m.mesh.normals[i * 3 + 1],
                        m.mesh.normals[i * 3 + 2],
                    ],
                });
            }

            let vertex_buffer = device
                .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
                .fill_from_slice(&vertices);

            let index_buffer = device
                .create_buffer_mapped(m.mesh.indices.len(), wgpu::BufferUsage::INDEX)
                .fill_from_slice(&m.mesh.indices);

            meshes.push(Mesh {
                name: m.name,
                vertex_buffer,
                index_buffer,
                num_elements: m.mesh.indices.len() as u32,
                material: m.mesh.material_id.unwrap_or(0),
            });
        }
        
        Ok((Self { meshes, materials, }, command_buffers))
    }
}

pub trait DrawModel {
    fn draw_mesh(&mut self, mesh: &Mesh, material: &Material, uniforms: &wgpu::BindGroup);
    fn draw_mesh_instanced(&mut self, mesh: &Mesh, material: &Material, instances: Range<u32>, uniforms: &wgpu::BindGroup);

    fn draw_model(&mut self, model: &Model, uniforms: &wgpu::BindGroup);
    fn draw_model_instanced(&mut self, model: &Model, instances: Range<u32>,  uniforms: &wgpu::BindGroup);
}

impl<'a> DrawModel for wgpu::RenderPass<'a> {
    fn draw_mesh(&mut self, mesh: &Mesh, material: &Material, uniforms: &wgpu::BindGroup) {
        self.draw_mesh_instanced(mesh, material, 0..1, uniforms);
    }

    fn draw_mesh_instanced(&mut self, mesh: &Mesh, material: &Material, instances: Range<u32>, uniforms: &wgpu::BindGroup) {
        self.set_vertex_buffers(0, &[(&mesh.vertex_buffer, 0)]);
        self.set_index_buffer(&mesh.index_buffer, 0);
        self.set_bind_group(0, &material.bind_group, &[]);
        self.set_bind_group(1, &uniforms, &[]);
        self.draw_indexed(0..mesh.num_elements, 0, instances);
    }

    fn draw_model(&mut self, model: &Model, uniforms: &wgpu::BindGroup) {
        self.draw_model_instanced(model, 0..1, uniforms);
    }

    fn draw_model_instanced(&mut self, model: &Model, instances: Range<u32>,  uniforms: &wgpu::BindGroup) {
        for mesh in &model.meshes {
            let material = &model.materials[mesh.material];
            self.draw_mesh_instanced(mesh, material, instances.clone(), uniforms);
        }
    }
}