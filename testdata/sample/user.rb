class User < ApplicationRecord
  has_many :posts

  def full_name
    "#{first_name} #{last_name}"
  end

  def self.find_active
    where(active: true)
  end
end

module Authentication
  def authenticate(password)
    BCrypt::Password.new(password_hash) == password
  end
end
