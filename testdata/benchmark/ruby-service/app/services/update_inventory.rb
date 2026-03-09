module Services
  class UpdateInventory
    def self.call(items:, **_rest)
      items.each do |item|
        deduct_stock(item[:product_id], item[:quantity])
      end
      { success: true }
    end

    def self.deduct_stock(product_id, quantity)
      # inventory update logic
    end
  end
end
