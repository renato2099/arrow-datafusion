// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Math expressions
use super::{ColumnarValue, ScalarValue};
use crate::error::{DataFusionError, Result};
use arrow::array::{Float32Array, Float64Array};
use arrow::datatypes::DataType;
use lazy_static::lazy_static;
use rand::{thread_rng, Rng};
use std::f64::INFINITY as INF;
use std::iter;
use std::sync::Arc;

macro_rules! downcast_compute_op {
    ($ARRAY:expr, $NAME:expr, $FUNC:ident, $TYPE:ident) => {{
        let n = $ARRAY.as_any().downcast_ref::<$TYPE>();
        match n {
            Some(array) => {
                let res: $TYPE =
                    arrow::compute::kernels::arity::unary(array, |x| x.$FUNC());
                Ok(Arc::new(res))
            }
            _ => Err(DataFusionError::Internal(format!(
                "Invalid data type for {}",
                $NAME
            ))),
        }
    }};
}

macro_rules! unary_primitive_array_op {
    ($VALUE:expr, $NAME:expr, $FUNC:ident) => {{
        match ($VALUE) {
            ColumnarValue::Array(array) => match array.data_type() {
                DataType::Float32 => {
                    let result = downcast_compute_op!(array, $NAME, $FUNC, Float32Array);
                    Ok(ColumnarValue::Array(result?))
                }
                DataType::Float64 => {
                    let result = downcast_compute_op!(array, $NAME, $FUNC, Float64Array);
                    Ok(ColumnarValue::Array(result?))
                }
                other => Err(DataFusionError::Internal(format!(
                    "Unsupported data type {:?} for function {}",
                    other, $NAME,
                ))),
            },
            ColumnarValue::Scalar(a) => match a {
                ScalarValue::Float32(a) => Ok(ColumnarValue::Scalar(
                    ScalarValue::Float32(a.map(|x| x.$FUNC())),
                )),
                ScalarValue::Float64(a) => Ok(ColumnarValue::Scalar(
                    ScalarValue::Float64(a.map(|x| x.$FUNC())),
                )),
                _ => Err(DataFusionError::Internal(format!(
                    "Unsupported data type {:?} for function {}",
                    ($VALUE).data_type(),
                    $NAME,
                ))),
            },
        }
    }};
}

macro_rules! math_unary_function {
    ($NAME:expr, $FUNC:ident) => {
        /// mathematical function that accepts f32 or f64 and returns f64
        pub fn $FUNC(args: &[ColumnarValue]) -> Result<ColumnarValue> {
            unary_primitive_array_op!(&args[0], $NAME, $FUNC)
        }
    };
}

math_unary_function!("sqrt", sqrt);
math_unary_function!("sin", sin);
math_unary_function!("cos", cos);
math_unary_function!("tan", tan);
math_unary_function!("asin", asin);
math_unary_function!("acos", acos);
math_unary_function!("atan", atan);
math_unary_function!("floor", floor);
math_unary_function!("ceil", ceil);
math_unary_function!("round", round);
math_unary_function!("trunc", trunc);
math_unary_function!("abs", abs);
math_unary_function!("signum", signum);
math_unary_function!("exp", exp);
math_unary_function!("ln", ln);
math_unary_function!("log2", log2);
math_unary_function!("log10", log10);

/// The maximum factorial representable
/// by a 64-bit floating point without
/// overflowing
pub const MAX_FACTORIAL: usize = 170;

/// Computes the factorial function `x -> x!` for
/// `170 >= x >= 0`. All factorials larger than `170!`
/// will overflow an `f64`.
///
/// # Remarks
///
/// Returns `f64::INFINITY` if `x > 170`
pub fn factorial_impl(x: u64) -> f64 {
    let x = x as usize;
    FCACHE.get(x).map_or(INF, |&fac| fac)
}

// Initialization for pre-computed cache of 171 factorial
// values 0!...170!
lazy_static! {
    static ref FCACHE: [f64; MAX_FACTORIAL + 1] = {
        let mut fcache = [1.0; MAX_FACTORIAL + 1];
        fcache
            .iter_mut()
            .enumerate()
            .skip(1)
            .fold(1.0, |acc, (i, elt)| {
                let fac = acc * i as f64;
                *elt = fac;
                fac
            });
        fcache
    };
}

/// factorial SQL function
pub fn factorial(args: &[ColumnarValue]) -> Result<ColumnarValue> {
    match &args[0] {
        ColumnarValue::Array(array) => {
            let x1 = array.as_any().downcast_ref::<Float64Array>();
            match x1 {
                Some(array) => {
                    let res: Float64Array =
                        arrow::compute::kernels::arity::unary(array, |x| {
                            factorial_impl(x as u64)
                        });
                    let arc1 = Arc::new(res);
                    Ok(ColumnarValue::Array(arc1))
                }
                _ => Err(DataFusionError::Internal(
                    "Invalid data type for factorial function".to_string(),
                )),
            }
        }
        _ => Err(DataFusionError::Internal(
            "Expect factorial function to take some params".to_string(),
        )),
    }
}

/// random SQL function
pub fn random(args: &[ColumnarValue]) -> Result<ColumnarValue> {
    let len: usize = match &args[0] {
        ColumnarValue::Array(array) => array.len(),
        _ => {
            return Err(DataFusionError::Internal(
                "Expect random function to take no param".to_string(),
            ))
        }
    };
    let mut rng = thread_rng();
    let values = iter::repeat_with(|| rng.gen_range(0.0..1.0)).take(len);
    let array = Float64Array::from_iter_values(values);
    Ok(ColumnarValue::Array(Arc::new(array)))
}

#[cfg(test)]
mod tests {

    use super::*;
    use arrow::array::{Float64Array, NullArray};

    #[test]
    fn test_random_expression() {
        let args = vec![ColumnarValue::Array(Arc::new(NullArray::new(1)))];
        let array = random(&args).expect("fail").into_array(1);
        let floats = array.as_any().downcast_ref::<Float64Array>().expect("fail");

        assert_eq!(floats.len(), 1);
        assert!(0.0 <= floats.value(0) && floats.value(0) < 1.0);
    }
}
