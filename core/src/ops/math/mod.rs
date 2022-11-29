#![allow(clippy::clone_on_copy)]

use super::array::MultiBroadcastTo;
use crate::internal::*;
use crate::ops::quant::scale_by;
use num_traits::bounds::Bounded;
use num_traits::int::PrimInt;
use num_traits::{Float, Zero};
use tract_data::internal::ClampCast;
pub use tract_data::prelude::round_ties_to_even;
use tract_linalg::{ScaleShiftAndRound, Scaler};
use tract_num_traits::AsPrimitive;

bin_to_super_type!(add, Add,
                   declutter: declutter_add,
                   linalg: Add,
                   validation: Validation::Rounding,
                   q: [i8, u8, i32, i32] => add_quant;
                   [f32, i8, i16, i32, i64, u8, u16, u32, u64, f16, f64, TDim] => |c, a, b| *c = a.clone() + b);

fn add_quant<T>(c: &mut T, a: &T, b: &T, zp: i32, _: f32)
where
    T: PrimInt + Bounded + AsPrimitive<i64> + Datum,
    i64: AsPrimitive<T>,
{
    *c = (a.as_() + b.as_() - zp as i64).clamp_cast()
}

bin_to_super_type!(sub, Sub,
                   declutter: declutter_sub,
                   linalg:Sub,
                   q: [i8, u8, i32, i32] => sub_quant;
                   [f32, i8, i16, i32, i64, u8, u16, u32, u64, f16, f64, TDim] => |c, a, b| *c = a.clone() - b);

fn sub_quant<T>(c: &mut T, a: &T, b: &T, zp: i32, _: f32)
where
    T: PrimInt + Bounded + AsPrimitive<i16> + Datum,
    i16: AsPrimitive<T>,
{
    *c = (a.as_() - b.as_() + zp as i16).clamp_cast()
}

bin_to_super_type!(mul, Mul,
                   cost: |dt| tvec!((Cost::FMA(dt), 1)),
                   declutter: declutter_mul,
                   linalg: Mul,
                   out_of_place: |c:&mut Tensor, a:&Tensor, b: &Tensor| -> TractResult<bool> {
                       if c.datum_type() == TDim::datum_type() &&
                           a.datum_type() == TDim::datum_type() && b.datum_type() == TDim::datum_type() {
                               let a = a.to_array_view::<TDim>()?;
                               let b = b.cast_to::<i32>()?;
                               let b = b.to_array_view::<i32>()?;
                               let c = c.to_array_view_mut::<TDim>()?;
                               crate::ndarray::Zip::from(c).and_broadcast(a).and_broadcast(b).for_each(|c,a,b| *c = a.clone() * *b);
                               Ok(true)
                           }
                       else {
                           match c.datum_type() {
                               DatumType::QI8(params) => {
                                   let (zp, scale) = params.zp_scale();
                                   let a = a.to_array_view::<i8>()?;
                                   let b = b.to_array_view::<i8>()?;
                                   let c = c.to_array_view_mut::<i8>()?;
                                   crate::ndarray::Zip::from(c)
                                       .and_broadcast(a)
                                       .and_broadcast(b)
                                       .for_each(|c,a,b| *c = scale_by((*a as i16 - zp as i16) * (*b as i16 - zp as i16) + zp as i16, scale).clamp_cast());
                                   Ok(true)
                               }
                               DatumType::QU8(params) => {
                                   let (zp, scale) = params.zp_scale();
                                   let a = a.to_array_view::<u8>()?;
                                   let b = b.to_array_view::<u8>()?;
                                   let c = c.to_array_view_mut::<u8>()?;
                                   crate::ndarray::Zip::from(c)
                                       .and_broadcast(a)
                                       .and_broadcast(b)
                                       .for_each(|c,a,b| *c = scale_by((*a as i32 - zp as i32) * (*b as i32 - zp as i32) + zp as i32, scale).clamp_cast());
                                   Ok(true)
                               }
                               _ => Ok(false)
                           }
                       }
                   },
                   q: [i8, u8, i32] => |c, a, b, _, _| *c = a.clone() * b;
[f32, i8, i16, i32, i64, u8, u16, u32, u64, f16, f64, TDim] => |c, a, b| *c = a.clone() * b
);

bin_to_super_type!(div, Div,
cost: |dt| tvec!((Cost::Div(dt), 1)),
declutter: declutter_div,
eval_override: |a:TValue, b: TValue| -> TractResult<Tensor> {
    if
        a.datum_type() == TDim::datum_type() && b.datum_type() == TDim::datum_type() {
            let a = a.to_array_view::<TDim>()?;
            let b = b.cast_to::<i32>()?;
            let b = b.to_array_view::<i32>()?;
            let c_shape = crate::broadcast::multi_broadcast(&[a.shape(), b.shape()]).context("no broadcast solution")?;
            unsafe {
                let mut c = Tensor::uninitialized_dt(DatumType::TDim, &c_shape)?;
                let view = c.to_array_view_mut::<TDim>()?;
                crate::ndarray::Zip::from(view).and_broadcast(a).and_broadcast(b).for_each(|c,a,b| *c = a.clone() / *b);
                Ok(c)
            }
        } else {
            Div.generic_eval(a,b)
        }
},
out_of_place: |c:&mut Tensor, a:&Tensor, b: &Tensor| -> TractResult<bool> {
    if c.datum_type() == TDim::datum_type() &&
        a.datum_type() == TDim::datum_type() && b.datum_type() == TDim::datum_type() {
            let a = a.to_array_view::<TDim>()?;
            let b = b.cast_to::<i32>()?;
            let b = b.to_array_view::<i32>()?;
            let c = c.to_array_view_mut::<TDim>()?;
            crate::ndarray::Zip::from(c).and_broadcast(a).and_broadcast(b).for_each(|c,a,b| *c = a.clone() / *b);
            Ok(true)
        } else if c.datum_type().is_quantized() || b.datum_type().is_quantized() || a.datum_type().is_quantized() {
            let a_f32 = a.cast_to::<f32>()?;
            let a_f32 = a_f32.to_array_view::<f32>()?;
            let b_f32 = b.cast_to::<f32>()?;
            let b_f32 = b_f32.to_array_view::<f32>()?;
            let c_f32 = &a_f32 / &b_f32;
            *c = c_f32.into_tensor().cast_to_dt(c.datum_type())?.into_owned();
            Ok(true)
        } else {
            Ok(false)
        }
},
[f32, i8, i16, i32, i64, u8, u16, u32, u64, f16, f64] => |c, a, b| *c = a.clone() / b
);

bin_to_super_type!(rem, Rem,
                                      eval_override: |a:TValue, b: TValue| -> TractResult<Tensor> {
                                          if
                                              a.datum_type() == TDim::datum_type() && b.datum_type() == TDim::datum_type() {
                                                  let a = a.to_array_view::<TDim>()?;
                                                  let b = b.cast_to::<i32>()?;
                                                  let b = b.to_array_view::<i32>()?;
                                                  let c_shape = crate::broadcast::multi_broadcast(&[a.shape(), b.shape()]).context("no broadcast solution")?;
                                                  unsafe {
                                                      let mut c = Tensor::uninitialized_dt(DatumType::TDim, &c_shape)?;
                                                      let view = c.to_array_view_mut::<TDim>()?;
                                                      crate::ndarray::Zip::from(view).and_broadcast(a).and_broadcast(b).for_each(|c,a,b| *c = a.clone() % *b);
                                                      Ok(c)
                                                  }
                                              } else {
                                                  Rem.generic_eval(a,b)
                                              }
                                      },
                                      out_of_place: |c:&mut Tensor, a:&Tensor, b: &Tensor| -> TractResult<bool> {
                                          if c.datum_type() == TDim::datum_type() &&
                                              a.datum_type() == TDim::datum_type() && b.datum_type() == TDim::datum_type() {
                                                  let a = a.to_array_view::<TDim>()?;
                                                  let b = b.cast_to::<i32>()?;
                                                  let b = b.to_array_view::<i32>()?;
                                                  let c = c.to_array_view_mut::<TDim>()?;
                                                  crate::ndarray::Zip::from(c).and_broadcast(a).and_broadcast(b).for_each(|c,a,b| *c = a.clone() % *b);
                                                  Ok(true)
                                              } else {
                                                  Ok(false)
                                              }
                                      },
                                      [f32, i8, i16, i32, i64, u8, u16, u32, u64, f16, f64] => |c, a, b| *c = a.clone() % b);

bin_to_super_type!(min, Min, linalg:Min,
                   operating_datum_type: super::logic::operating_datum_type_for_cmp,
                   q: [i8, u8, i32] => |c, a, b, _, _| *c = if a < b { *a } else { *b };
                   [f16, f32, f64] => |c,a,b| *c = a.min(*b),
                   [i8, i16, i32, i64, u8, u16, u32, u64] => |c, a, b| *c = *a.min(b));
bin_to_super_type!(max, Max, linalg:Max,
                   operating_datum_type: super::logic::operating_datum_type_for_cmp,
                   q: [i8, u8, i32] => |c, a, b, _, _| *c = if a < b { *b } else { *a };
                   [f16, f32, f64] => |c,a,b| *c = a.max(*b),
                   [i8, i16, i32, i64, u8, u16, u32, u64] => |c, a, b| *c = *a.max(b));

bin_to_super_type!(pow, Pow,
                   declutter: declutter_pow,
                   [f32, f64] => |c,a,b| *c = a.powf(*b),
                   [i32, i64] => |c,a,b| *c = a.pow(*b as u32));

bin_to_super_type!(shift_left, ShiftLeft,
                   [i8, i16, i32, i64, u8, u16, u32, u64] => |c, a, b| *c = *a << *b);
bin_to_super_type!(shift_right, ShiftRight,
                   [i8, i16, i32, i64, u8, u16, u32, u64] => |c, a, b| *c = *a >> *b);

struct OneUniformInput {
    uni: Arc<Tensor>,
    var: OutletId,
    left_is_uniform: bool,
}

fn one_input_is_uniform(
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<OneUniformInput>> {
    if let &[a, b] = &*model.node_input_facts(node.id)? {
        if let Some(a) = &a.uniform {
            return Ok(Some(OneUniformInput {
                uni: a.clone(),
                var: node.inputs[1],
                left_is_uniform: true,
            }));
        } else if let Some(b) = &b.uniform {
            return Ok(Some(OneUniformInput {
                uni: b.clone(),
                var: node.inputs[0],
                left_is_uniform: false,
            }));
        }
    }
    return Ok(None);
}

fn declutter_neutral(
    model: &TypedModel,
    node: &TypedNode,
    value: i64,
    also_left: bool,
) -> TractResult<Option<TypedModelPatch>> {
    if let Some(uniform) = one_input_is_uniform(model, node)? {
        let integer = uniform.uni.cast_to_scalar::<i64>()?;
        if tensor0(integer)
            .cast_to_dt(uniform.uni.datum_type())?
            .close_enough(&uniform.uni, false)
            .is_ok()
        {
            if integer == value && (also_left || !uniform.left_is_uniform) {
                return Ok(Some(TypedModelPatch::rewire(
                    model,
                    &[uniform.var],
                    &[node.id.into()],
                    &|_, inputs| Ok(inputs.into()),
                )?));
            }
        }
    }
    return Ok(None);
}

fn declutter_add(
    _op: &Add,
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<TypedModelPatch>> {
    declutter_neutral(model, node, 0, true)
}

fn declutter_sub(
    _op: &Sub,
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<TypedModelPatch>> {
    declutter_neutral(model, node, 0, false)
}

fn declutter_mul(
    _op: &Mul,
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<TypedModelPatch>> {
    if let Some(p) = declutter_neutral(model, node, 1, true)? {
        return Ok(Some(p));
    }
    if let Some(uniform) = one_input_is_uniform(model, node)? {
        let var_fact = model.outlet_fact(uniform.var)?;
        if uniform.uni.cast_to_scalar::<f64>()? == 0.0 {
            let shapes =
                model.node_input_facts(node.id)?.iter().map(|f| &f.shape).collect::<TVec<_>>();
            let shape: ShapeFact =
                crate::broadcast::multi_broadcast(&shapes).context("Failed to broadcast")?.into();
            return Ok(Some(TypedModelPatch::rewire(
                model,
                &[],
                &[node.id.into()],
                &|patch, _| {
                    let scalar =
                        patch.add_const(format!("{}.zero", node.name), uniform.uni.clone())?;
                    let op = MultiBroadcastTo::new(shape.clone());
                    patch.wire_node(&node.name, op, &[scalar])
                },
            )?));
        }
        let dt = uniform.uni.datum_type();
        let integer = uniform.uni.cast_to_scalar::<i64>()?;
        if tensor0(integer)
            .cast_to_dt(uniform.uni.datum_type())?
            .close_enough(&uniform.uni, false)
            .is_ok()
        {
            if dt.is_integer() && uniform.uni.cast_to_scalar::<i64>()?.count_ones() == 1 {
                let shift = integer.trailing_zeros();
                return Ok(Some(TypedModelPatch::rewire(
                    model,
                    &[uniform.var],
                    &[node.id.into()],
                    &|patch, inputs| {
                        let shift = patch.add_const(
                            format!("{}.shift", node.name),
                            tensor0(shift)
                                .cast_to_dt(dt)?
                                .into_owned()
                                .broadcast_into_rank(var_fact.rank())?,
                        )?;
                        patch.wire_node(&node.name, shift_left(), &[inputs[0], shift])
                    },
                )?));
            }
        }
    }
    return Ok(None);
}

fn declutter_div(
    _op: &Div,
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<TypedModelPatch>> {
    if let Some(p) = declutter_neutral(model, node, 1, false)? {
        return Ok(Some(p));
    }
    if let Some(uniform) = one_input_is_uniform(model, node)? {
        let var_fact = model.outlet_fact(uniform.var)?;
        let dt = uniform.uni.datum_type();
        let integer = uniform.uni.cast_to_scalar::<i64>()?;
        if tensor0(integer)
            .cast_to_dt(uniform.uni.datum_type())?
            .close_enough(&uniform.uni, false)
            .is_ok()
        {
            if dt.is_integer()
                && !uniform.left_is_uniform
                && uniform.uni.cast_to_scalar::<i64>()?.count_ones() == 1
            {
                let shift = integer.trailing_zeros();
                return Ok(Some(TypedModelPatch::rewire(
                    model,
                    &[uniform.var],
                    &[node.id.into()],
                    &|patch, inputs| {
                        let shift = patch.add_const(
                            format!("{}.shift", node.name),
                            tensor0(shift)
                                .cast_to_dt(dt)?
                                .into_owned()
                                .broadcast_into_rank(var_fact.rank())?,
                        )?;
                        patch.wire_node(&node.name, shift_right(), &[inputs[0], shift])
                    },
                )?));
            }
        }
    } else if model.outlet_fact(node.inputs[0])?.datum_type.is_float() {
        return Ok(Some(TypedModelPatch::rewire(
            model,
            &node.inputs,
            &[node.id.into()],
            &|patch, inputs| {
                let denum =
                    patch.wire_node(format!("{}-recip", node.name), recip(), &[inputs[1]])?[0];
                patch.wire_node(&node.name, mul(), &[inputs[0], denum])
            },
        )?));
    }
    return Ok(None);
}

fn declutter_pow(
    _op: &Pow,
    model: &TypedModel,
    node: &TypedNode,
) -> TractResult<Option<TypedModelPatch>> {
    if let Some(p) = declutter_neutral(model, node, 1, false)? {
        return Ok(Some(p));
    }
    let b = model.outlet_fact(node.inputs[1])?;
    if let Some(b) = &b.uniform {
        let b = b.cast_to_scalar::<f32>()?;
        if b == 2.0 {
            return Ok(Some(TypedModelPatch::replace_single_op(
                model,
                node,
                &[node.inputs[0]],
                square(),
            )?));
        } else if b == 3.0 {
            return Ok(Some(TypedModelPatch::replace_single_op(
                model,
                node,
                &[node.inputs[0]],
                cube(),
            )?));
        } else if b == 0.5 {
            return Ok(Some(TypedModelPatch::replace_single_op(
                model,
                node,
                &[node.inputs[0]],
                sqrt(),
            )?));
        }
    }
    Ok(None)
}

element_wise!(abs, Abs, [i8, i16, i32, i64, f16, f32, i32] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.abs());
    Ok(())
};
q: [i8, u8, i32, i32] => f32::abs;
operating_datum_type: |dt| if dt == TDim::datum_type() { i64::datum_type() } else { dt }
);

element_wise!(exp, Exp, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.exp());
    Ok(())
};
q: [i8, u8, i32, i32] => f32::exp;
validation: Validation::Rounding
);

element_wise!(ln, Ln, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.ln());
    Ok(())
};
q: [i8, u8, i32, i32] => f32::ln;
validation: Validation::Rounding
);

element_wise!(square, Square, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.powi(2));
    Ok(())
};
q: [i8, u8, i32, i32] => |f : f32| f.powi(2);
validation: Validation::Rounding
);

element_wise!(cube, Cube, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.powi(3));
    Ok(())
};
q: [i8, u8, i32, i32] => |f : f32| f.powi(3);
validation: Validation::Rounding
);

element_wise!(sqrt, Sqrt, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.sqrt());
    Ok(())
};
q: [i8, u8, i32, i32] => f32::sqrt;
validation: Validation::Rounding
);

element_wise!(recip, Recip, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.recip());
    Ok(())
};
q: [i8, u8, i32, i32] => f32::recip;
cost: |dt| {tvec!((Cost::Div(dt), 1))};
declutter: declutter_recip;
validation: Validation::Rounding
);

fn declutter_recip(model: &TypedModel, node: &TypedNode) -> TractResult<Option<TypedModelPatch>> {
    use super::element_wise::*;
    if let Some(prec) = model.single_prec(node.id)? {
        if let Some(ew) = prec.op_as::<ElementWiseOp>() {
            let repl = if ew.0.is::<Sqrt>() {
                Some(rsqrt())
            } else if ew.0.is::<Rsqrt>() {
                Some(sqrt())
            } else {
                None
            };
            if let Some(repl) = repl {
                let mut patch = TypedModelPatch::default();
                let mut wire = patch.tap_model(model, prec.inputs[0])?;
                wire = patch.wire_node(&node.name, repl, &[wire])?[0];
                patch.shunt_outside(model, node.id.into(), wire)?;
                return Ok(Some(patch));
            }
        }
    }
    Ok(None)
}

element_wise!(rsqrt, Rsqrt, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.sqrt().recip());
    Ok(())
};
q: [i8, u8, i32] => |x : f32| x.sqrt().recip();
validation: Validation::Rounding
);

element_wise!(ceil, Ceil, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.ceil());
    Ok(())
};
q: [i8, u8, i32] => f32::recip);

element_wise!(floor, Floor, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.floor());
    Ok(())
};
q: [i8, u8, i32] => f32::floor);

element_wise!(round, Round, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.round());
    Ok(())
};
q: [i8, u8, i32] => f32::round);

element_wise!(q_scale, QScale{scaler: Scaler},[i32] => |op, xs| {
    xs.iter_mut().for_each(|x| *x = x.q_scale(op.scaler));
    Ok(())
});

element_wise!(round_half_to_even, RoundHalfToEven,[ f32] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = round_ties_to_even(*x));
    Ok(())
};
q: [i8, u8, i32] => round_ties_to_even);

element_wise!(cos, Cos, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.cos());
    Ok(())
};
q: [i8, u8, i32] => f32::cos);

element_wise!(sin, Sin, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.sin());
    Ok(())
};
q: [i8, u8, i32] => f32::sin);

element_wise!(tan, Tan, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.tan());
    Ok(())
};
q: [i8, u8, i32] => f32::tan);

element_wise!(acos, Acos, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.acos());
    Ok(())
};
q: [i8, u8, i32] => f32::acos);

element_wise!(asin, Asin, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.asin());
    Ok(())
};
q: [i8, u8, i32] => f32::asin);

element_wise!(atan, Atan, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.atan());
    Ok(())
};
q: [i8, u8, i32] => f32::atan);

element_wise!(cosh, Cosh, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.cosh());
    Ok(())
};
q: [i8, u8, i32] => f32::cosh);

element_wise!(sinh, Sinh, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.sinh());
    Ok(())
};
q: [i8, u8, i32] => f32::sinh);

element_wise!(tanh, Tanh,
 [f16] => |_, xs| { (tract_linalg::ops().tanh_f16)().run(xs) },
 [f32] => |_, xs| { (tract_linalg::ops().tanh_f32)().run(xs) },
 [f64] => |_, xs| { xs.iter_mut().for_each(|x| *x = x.tanh()); Ok(()) };
 q: [i8, u8, i32] => f32::tanh;
 cost: |dt| {tvec!((Cost::FMA(dt), 11), (Cost::Div(dt), 1))}
);

element_wise!(acosh, Acosh, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.acosh());
    Ok(())
};
q: [i8, u8, i32] => f32::acosh);
element_wise!(asinh, Asinh, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.asinh());
    Ok(())
};
q: [i8, u8, i32] => f32::asinh);
element_wise!(atanh, Atanh, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = x.atanh());
    Ok(())
};
q: [i8, u8, i32] => f32::atanh);

element_wise!(neg, Neg, [i8, i16, i32, i64, f16, f32, f64, TDim] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = -x.clone());
    Ok(())
};
q: [i8, u8, i32] => |x: f32| -x);

element_wise!(sign, Sign, [f16, f32, f64] => |_, xs| {
    xs.iter_mut().for_each(|x| *x = if x.is_zero() { *x } else { x.signum() });
    Ok(())
};
q: [i8, u8, i32] => f32::signum);

#[cfg(test)]
mod tests {
    use crate::ops::binary::TypedBinOp;

    use super::*;
    use ndarray::arr2;

    #[test]
    fn test_mul() {
        let a = arr2(&[[1., 2.], [3., 4.]]);
        let b = arr2(&[[1., 0.], [0., 0.]]);
        assert_eq!(a * b, arr2(&[[1., 0.], [0., 0.]]));
    }

    #[test]
    fn dot() {
        let a = arr2(&[[1., 2.], [3., 4.]]);
        let b = arr2(&[[1., 0.], [0., 0.]]);
        assert_eq!(a.dot(&b), arr2(&[[1., 0.], [3., 0.]]));
    }

    #[test]
    fn mul_as_shift_left() -> TractResult<()> {
        let mut model = TypedModel::default();
        let x = model.add_source("x", i32::fact([2usize, 2]))?;
        let a = model.add_const("a", tensor0(4i32).broadcast_into_rank(2)?.into_arc_tensor())?;
        let y = model.wire_node("y", mul(), &[x, a])?[0];
        model.set_output_outlets(&[y])?;
        let result = SimplePlan::new(&model)?.run(tvec!(tensor2(&[[1, 2], [3, 4]]).into()))?;
        assert_eq!(*result[0], tensor2(&[[4, 8], [12, 16]]));
        let decluttered = model.into_decluttered()?;
        let result =
            SimplePlan::new(&decluttered)?.run(tvec!(tensor2(&[[1, 2], [3, 4]]).into()))?;
        assert_eq!(*result[0], tensor2(&[[4, 8], [12, 16]]));
        let op = decluttered
            .node(decluttered.output_outlets()?[0].node)
            .op()
            .downcast_ref::<TypedBinOp>()
            .unwrap();
        assert!(op.0.downcast_ref::<ShiftLeft>().is_some());
        Ok(())
    }

    #[test]
    fn div_as_shift() -> TractResult<()> {
        let mut model = TypedModel::default();
        let x = model.add_source("a", i32::fact([2usize, 2]))?;
        let s = model.add_const("shift", tensor2(&[[4]]))?;
        let y = model.wire_node("c", div(), [x, s].as_ref())?[0];
        model.set_output_outlets(&[y])?;
        let result = SimplePlan::new(&model)?.run(tvec!(tensor2(&[[16, 32], [64, 68]]).into()))?;
        assert_eq!(*result[0], tensor2(&[[4, 8], [16, 17]]));
        let decluttered = model.into_decluttered()?;
        let result =
            SimplePlan::new(&decluttered)?.run(tvec!(tensor2(&[[16, 32], [64, 68]]).into()))?;
        assert_eq!(*result[0], tensor2(&[[4, 8], [16, 17]]));
        let op = decluttered
            .node(decluttered.output_outlets()?[0].node)
            .op()
            .downcast_ref::<TypedBinOp>()
            .unwrap();
        assert!(op.0.downcast_ref::<ShiftRight>().is_some());
        Ok(())
    }
}
