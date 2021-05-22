// --prove                  Benchmark prover
// --verify                 Benchmark verifier
// --proofs <num>           Sets number of proofs in a batch
// --public <num>           Sets number of public inputs
// --private <num>          Sets number of private inputs
// --gpu                    Enables GPU
// --samples                Number of runs
// --dummy                  Skip param generation and generate dummy params/proofs
use std::sync::Arc;
use std::time::Instant;

use crusty3_zk::groth16::{
    create_random_proof_batch, generate_random_parameters, prepare_verifying_key,
    verify_proofs_batch, Parameters, Proof, VerifyingKey,
};
use crusty3_zk::{
    bls::{Bls12, Engine, Fr},
    Circuit, ConstraintSystem, SynthesisError,
};
use fff::{Field, PrimeField, ScalarEngine};
use groupy::CurveProjective;
use rand::{thread_rng, Rng};
use structopt::StructOpt;

macro_rules! timer {
    ($e:expr) => {{
        let before = Instant::now();
        let ret = $e;
        (
            ret,
            (before.elapsed().as_secs() * 1000 as u64 + before.elapsed().subsec_millis() as u64),
        )
    }};
}

#[derive(Clone)]
pub struct DummyDemo {
    pub public: usize,
    pub private: usize,
}

impl<E: Engine> Circuit<E> for DummyDemo {
    fn synthesize<CS: ConstraintSystem<E>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
        assert!(self.public >= 1);
        let mut x_val = E::Fr::from_str("2");
        let mut x = cs.alloc_input(|| "", || x_val.ok_or(SynthesisError::AssignmentMissing))?;
        let mut pubs = 1;

        for _ in 0..self.private + self.public - 1 {
            // Allocate: x * x = x2
            let x2_val = x_val.map(|mut e| {
                e.square();
                e
            });

            let x2 = if pubs < self.public {
                pubs += 1;
                cs.alloc_input(|| "", || x2_val.ok_or(SynthesisError::AssignmentMissing))?
            } else {
                cs.alloc(|| "", || x2_val.ok_or(SynthesisError::AssignmentMissing))?
            };

            // Enforce: x * x = x2
            cs.enforce(|| "", |lc| lc + x, |lc| lc + x, |lc| lc + x2);

            x = x2;
            x_val = x2_val;
        }

        cs.enforce(
            || "",
            |lc| lc + (x_val.unwrap(), CS::one()),
            |lc| lc + CS::one(),
            |lc| lc + x,
        );

        Ok(())
    }
}

fn random_points<C: CurveProjective, R: Rng>(count: usize, rng: &mut R) -> Vec<C::Affine> {
    // Number of distinct points is limited because generating random points is very time
    // consuming, so it's better to just repeat them.
    const DISTINT_POINTS: usize = 100;
    (0..DISTINT_POINTS)
        .map(|_| C::random(rng).into_affine())
        .collect::<Vec<_>>()
        .into_iter()
        .cycle()
        .take(count)
        .collect()
}

fn dummy_proofs<E: Engine, R: Rng>(count: usize, rng: &mut R) -> Vec<Proof<E>> {
    (0..count)
        .map(|_| Proof {
            a: E::G1::random(rng).into_affine(),
            b: E::G2::random(rng).into_affine(),
            c: E::G1::random(rng).into_affine(),
        })
        .collect()
}

fn dummy_inputs<E: Engine, R: Rng>(count: usize, rng: &mut R) -> Vec<<E as ScalarEngine>::Fr> {
    (0..count)
        .map(|_| <E as ScalarEngine>::Fr::random(rng))
        .collect()
}

fn dummy_vk<E: Engine, R: Rng>(public: usize, rng: &mut R) -> VerifyingKey<E> {
    VerifyingKey {
        alpha_g1: E::G1::random(rng).into_affine(),
        beta_g1: E::G1::random(rng).into_affine(),
        beta_g2: E::G2::random(rng).into_affine(),
        gamma_g2: E::G2::random(rng).into_affine(),
        delta_g1: E::G1::random(rng).into_affine(),
        delta_g2: E::G2::random(rng).into_affine(),
        ic: random_points::<E::G1, _>(public + 1, rng),
    }
}

fn dummy_params<E: Engine, R: Rng>(public: usize, private: usize, rng: &mut R) -> Parameters<E> {
    let count = public + private;
    let hlen = (1 << (((count + public + 1) as f64).log2().ceil() as usize)) - 1;
    Parameters {
        vk: dummy_vk(public, rng),
        h: Arc::new(random_points::<E::G1, _>(hlen, rng)),
        l: Arc::new(random_points::<E::G1, _>(private, rng)),
        a: Arc::new(random_points::<E::G1, _>(count, rng)),
        b_g1: Arc::new(random_points::<E::G1, _>(count, rng)),
        b_g2: Arc::new(random_points::<E::G2, _>(count, rng)),
    }
}

#[derive(Debug, StructOpt, Clone, Copy)]
#[structopt(name = "Bellman Bench", about = "Benchmarking Bellman.")]
struct Opts {
    #[structopt(long = "proofs", default_value = "1")]
    proofs: usize,
    #[structopt(long = "public", default_value = "1")]
    public: usize,
    #[structopt(long = "private", default_value = "1000000")]
    private: usize,
    #[structopt(long = "samples", default_value = "10")]
    samples: usize,
    #[structopt(long = "gpu")]
    gpu: bool,
    #[structopt(long = "verify")]
    verify: bool,
    #[structopt(long = "prove")]
    prove: bool,
    #[structopt(long = "dummy")]
    dummy: bool,
}

// fn main() {
//     let rng = &mut thread_rng();
//     pretty_env_logger::init_timed();

//     let opts = Opts::from_args();
//     if opts.gpu {
//         std::env::set_var("BELLMAN_VERIFIER", "gpu");
//     } else {
//         std::env::set_var("BELLMAN_NO_GPU", "1");
//     }

//     let circuit = DummyDemo {
//         public: opts.public,
//         private: opts.private,
//     };
//     let circuits = vec![circuit.clone(); opts.proofs];

//     let params = if opts.dummy {
//         dummy_params::<Bls12, _>(opts.public, opts.private, rng)
//     } else {
//         println!("Generating params... (You can skip this by passing `--dummy` flag)");
//         generate_random_parameters(circuit.clone(), rng).unwrap()
//     };
//     let pvk = prepare_verifying_key(&params.vk);

//     if opts.prove {
//         println!("Proving...");

//         for _ in 0..opts.samples {
//             let (_, took) =
//                 timer!(create_random_proof_batch(circuits.clone(), &params, rng).unwrap());
//             println!("Proof generation finished in {}ms", took);
//         }
//     }

//     if opts.verify {
//         println!("Verifying...");

//         let (inputs, proofs) = if opts.dummy {
//             (
//                 dummy_inputs::<Bls12, _>(opts.public, rng),
//                 dummy_proofs::<Bls12, _>(opts.proofs, rng),
//             )
//         } else {
//             let mut inputs = Vec::new();
//             let mut num = Fr::one();
//             num.double();
//             for _ in 0..opts.public {
//                 inputs.push(num);
//                 num.square();
//             }
//             println!("(Generating valid proofs...)");
//             let proofs = create_random_proof_batch(circuits.clone(), &params, rng).unwrap();
//             (inputs, proofs)
//         };

//         let vk = params.vk;

//         println!("Print alpha_g1 verification key: {}", vk.alpha_g1);
//         println!("Print beta_g1 verification key: {}", vk.beta_g1);
//         println!("Print beta_g2 verification key: {}", vk.beta_g2);
//         println!("Print gamma_g2 verification key: {}", vk.gamma_g2);
//         println!("Print delta_g1 verification key: {}", vk.delta_g1);
//         println!("Print delta_g2 verification key: {}", vk.delta_g2);
//         //println!("Print ic verification key: {}", vk.ic);

//         let mut v = vec![];
//         vk.write(&mut v).unwrap();

//         println!("Proof vector size: {}", v.len());
//         println!("{:02x?}", v);

//         println!("Print a after proof creation: {}", proofs[0].a);
//         println!("Print b after proof creation: {}", proofs[0].b);
//         println!("Print c after proof creation: {}", proofs[0].c);

//         let mut v = vec![];
//         proofs[0].write(&mut v).unwrap();

//         println!("Proof vector size: {}", v.len());
//         println!("{:01x?}", v);

//         let de_prf = Proof::<Bls12>::read(&v[..]).unwrap();

//         println!("Print a after proof decoding: {}", de_prf.a);
//         println!("Print b after proof decoding: {}", de_prf.b);
//         println!("Print c after proof decoding: {}", de_prf.c);

//         for _ in 0..opts.samples {
//             let pref = proofs.iter().collect::<Vec<&_>>();
//             println!(
//                 "{} proofs, each having {} public inputs...",
//                 opts.proofs, opts.public
//             );
//             let (valid, took) = timer!(verify_proofs_batch(
//                 &pvk,
//                 rng,
//                 &pref[..],
//                 &vec![inputs.clone(); opts.proofs]
//             )
//             .unwrap());
//             println!("Verification finished in {}ms (Valid: {})", took, valid);
//         }
//     }
// }

fn get_file_as_byte_vec(filename: &String) -> Vec<u8> {

    use std::fs::File;
    use std::io::Read;
    use std::fs;

    let mut f = File::open(&filename).expect("no file found");
    let metadata = fs::metadata(&filename).expect("unable to read metadata");
    let mut buffer = vec![0; metadata.len() as usize];
    f.read(&mut buffer).expect("buffer overflow");

    buffer
}

fn main() {
    
    use crusty3_zk::bls::{Bls12, Fr, Fq, FqRepr};
    use crusty3_zk::groth16::{fp_process, groth16_proof_from_byteblob};
    use std::fs::read;
    use groupy::{CurveAffine, EncodedPoint};

    extern crate serde_json;

    let mut byteblob = std::fs::read("data.bin").unwrap();

    let g1_byteblob_size = <<crusty3_zk::bls::Bls12 as Engine>::G1Affine as CurveAffine>::Compressed::size();
    let g2_byteblob_size = <<crusty3_zk::bls::Bls12 as Engine>::G2Affine as CurveAffine>::Compressed::size();

    let proof_byteblob_size = g1_byteblob_size + g2_byteblob_size + g1_byteblob_size;

    // let de_prf = Proof::<Bls12>::read(&byteblob[..proof_byteblob_size]).unwrap();

    let de_prf = groth16_proof_from_byteblob::<Bls12>(&byteblob[..proof_byteblob_size]).unwrap();

    println!("Print a after proof decoding: {}, size in byteblob: {}", de_prf.a, g1_byteblob_size);
    println!("Print b after proof decoding: {}, size in byteblob: {}", de_prf.b, g2_byteblob_size);
    println!("Print c after proof decoding: {}, size in byteblob: {}", de_prf.c, g1_byteblob_size);

    println!("Overall proof size in byteblob: {}", proof_byteblob_size);

    // let arr = [
    //         0x2058eebaac3db022u64,
    //         0xd8f94159af393618u64,
    //         0x4e041f53ff779974u64,
    //         0x03a5f678559fecdcu64,
    //         0xcdb85eca3da1f440u64,
    //         0x006d55d738a89daau64,
    //     ];

    // let example_fp = Fq::from_repr(FqRepr(arr)).unwrap();

    // println!("Print example_fp before coding: {}", example_fp);

    // use byteorder::{ByteOrder, BigEndian, LittleEndian};

    // let c2 = vec![
    //         0x20u8, 0x58u8, 0xeeu8, 0xbau8, 0xacu8, 0x3du8, 0xb0u8, 0x22u8, 
    //         0xd8u8, 0xf9u8, 0x41u8, 0x59u8, 0xafu8, 0x39u8, 0x36u8, 0x18u8,
    //         0x4eu8, 0x04u8, 0x1fu8, 0x53u8, 0xffu8, 0x77u8, 0x99u8, 0x74u8,
    //         0x03u8, 0xa5u8, 0xf6u8, 0x78u8, 0x55u8, 0x9fu8, 0xecu8, 0xdcu8,
    //         0xcdu8, 0xb8u8, 0x5eu8, 0xcau8, 0x3du8, 0xa1u8, 0xf4u8, 0x40u8,
    //         0x00u8, 0x6du8, 0x55u8, 0xd7u8, 0x38u8, 0xa8u8, 0x9du8, 0xaau8,
    //     ];
    
    let fp_byteblob_size = 48;
    let fp_byteblob : Vec<u8> = byteblob[proof_byteblob_size..proof_byteblob_size+fp_byteblob_size].to_vec();

    println!("Print c2 before coding: {:02x?}", fp_byteblob);

    // let rdr = vec![1, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0];
    // let mut dst = [0; 6];
    // LittleEndian::read_u64_into(&fp_byteblob, &mut dst);

    //println!("Print c2 u64 array before decoding: {:016x?}", dst);
    // assert_eq!([1,2,4], dst);
    // let mut bytes = [0; 6*8];
    // BigEndian::write_u64_into(&dst, &mut bytes);
    // assert_eq!(c2, bytes);

    // println!("Print c2 after decoding: {:02x?}", bytes.to_vec());

    //let c21 = Fq::from_repr(FqRepr(dst)).unwrap();

    let c21 = fp_process::<Bls12>(&byteblob[proof_byteblob_size..proof_byteblob_size+fp_byteblob_size]).unwrap();

    println!("Print c21 after decoding: {}", c21);

}