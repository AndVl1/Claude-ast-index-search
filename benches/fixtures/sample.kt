package com.example.payments

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject
import javax.inject.Singleton

sealed class PaymentState {
    object Idle : PaymentState()
    data class Loading(val orderId: String) : PaymentState()
    data class Success(val receipt: Receipt) : PaymentState()
    data class Error(val message: String, val code: Int) : PaymentState()
}

data class Receipt(
    val id: String,
    val amount: Double,
    val currency: String,
    val timestamp: Long,
)

interface PaymentRepository {
    suspend fun processPayment(orderId: String, amount: Double): Result<Receipt>
    suspend fun getHistory(userId: String): Flow<List<Receipt>>
    suspend fun cancel(orderId: String): Result<Unit>
}

interface PaymentApi {
    suspend fun charge(orderId: String, amount: Double): Receipt
    suspend fun refund(orderId: String): Receipt
}

interface PaymentCache {
    suspend fun save(receipt: Receipt)
    fun getHistory(userId: String): Flow<List<Receipt>>
    suspend fun clear()
}

@Singleton
class PaymentRepositoryImpl @Inject constructor(
    private val api: PaymentApi,
    private val cache: PaymentCache,
) : PaymentRepository {

    private val _state = MutableStateFlow<PaymentState>(PaymentState.Idle)
    val state = _state.asStateFlow()

    override suspend fun processPayment(orderId: String, amount: Double): Result<Receipt> {
        _state.value = PaymentState.Loading(orderId)
        return try {
            val receipt = api.charge(orderId, amount)
            cache.save(receipt)
            _state.value = PaymentState.Success(receipt)
            Result.success(receipt)
        } catch (e: Exception) {
            _state.value = PaymentState.Error(e.message ?: "unknown", -1)
            Result.failure(e)
        }
    }

    override suspend fun getHistory(userId: String): Flow<List<Receipt>> {
        return cache.getHistory(userId)
    }

    override suspend fun cancel(orderId: String): Result<Unit> {
        return try {
            api.refund(orderId)
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    companion object {
        const val MAX_RETRY = 3
        const val DEFAULT_CURRENCY = "USD"
    }
}

class PaymentViewModel(private val repo: PaymentRepository) {
    suspend fun charge(orderId: String, amount: Double) {
        repo.processPayment(orderId, amount)
    }
}
